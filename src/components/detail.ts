import { canRenderFillButton } from "../handlers/domains/limit-order-guards.ts";
import {
  EXECUTION_FEE_RATE,
  SATS_PER_FULL_CONTRACT,
  state,
  WIN_FEE_RATE,
} from "../state.ts";
import type { Market } from "../types.ts";
import { hexToNpub } from "../utils/crypto.ts";
import {
  formatBlockHeight,
  formatProbabilityWithPercent,
  formatSats,
  formatSettlementDateTime,
} from "../utils/format.ts";
import { escapeAttr, escapeHtml } from "../utils/html.ts";
import {
  clampContractPriceSats,
  getAvailableOrderContracts,
  getEstimatedSettlementDate,
  getLimitOrdersForSide,
  getOrderbookLevels,
  getPathAvailability,
  getPositionContracts,
  getQuoteEffectivePriceContractsPerSat,
  getQuoteEffectivePriceSatsPerContract,
  getQuoteRemainingSeconds,
  getSelectedMarket,
  getTradePreview,
  isExpired,
  stateBadge,
  stateLabel,
} from "../utils/market.ts";
import { chartSkeleton } from "./market-chart.ts";
export function renderPathCard(
  label: string,
  enabled: boolean,
  formula: string,
  next: string,
): string {
  return `<div class="rounded-lg border px-3 py-2 ${
    enabled
      ? "border-slate-700 bg-slate-900/50 text-slate-300"
      : "border-slate-800 bg-slate-950/60 text-slate-400"
  }">
    <div class="mb-1 flex items-center justify-between gap-2">
      <p class="text-sm font-medium">${label}</p>
      <span class="status-chip ${enabled ? "border border-emerald-500/40 bg-emerald-500/10 text-emerald-300" : "border border-slate-700 bg-slate-800/60 text-slate-400"}">${enabled ? "Available" : "Locked"}</span>
    </div>
    <p class="text-xs">${formula}</p>
    <p class="mt-1 text-xs">${next}</p>
  </div>`;
}

export function renderActionTicket(market: Market): string {
  const paths = getPathAvailability(market);
  const expired = isExpired(market);
  const preview = getTradePreview(market);
  const executionPriceSats = Math.round(preview.executionPriceSats);
  const positions = getPositionContracts(market);
  const isBuy = state.tradeIntent === "open";
  const isSellLimit = !isBuy && state.orderType === "limit";
  const parsedSellDraftLots = Math.max(
    0,
    Math.floor(Number(state.tradeContractsDraft || "0") || 0),
  );
  const hasSellInput = parsedSellDraftLots >= 1;
  const selectedPositionContracts =
    state.selectedSide === "yes" ? positions.yes : positions.no;
  const availableSellLots = Math.max(0, Math.floor(selectedPositionContracts));
  const hasSellBalance = availableSellLots >= 1;
  const fillabilityLabel =
    state.orderType === "limit"
      ? preview.fill.filledContracts <= 0
        ? "Resting only (not fillable now)"
        : preview.fill.isPartial
          ? "Partially fillable now"
          : "Fully fillable now"
      : preview.fill.isPartial
        ? "May partially fill"
        : "Expected to fill now";
  const sideOrders = getLimitOrdersForSide(market, state.selectedSide);
  const limitSellWarning = isSellLimit ? state.limitSellWarning : null;
  const limitSellWarningInfo = isSellLimit ? state.limitSellWarningInfo : "";

  const issueCollateral = state.pairsInput * 2 * market.cptSats;
  const cancelCollateral = state.pairsInput * 2 * market.cptSats;
  const redeemRate = paths.redeem
    ? 2 * market.cptSats
    : paths.expiryRedeem
      ? market.cptSats
      : 0;
  const redeemCollateral = state.tokensInput * redeemRate;
  const yesDisplaySats = clampContractPriceSats(
    Math.round((market.yesPrice ?? 0.5) * SATS_PER_FULL_CONTRACT),
  );
  const noDisplaySats = SATS_PER_FULL_CONTRACT - yesDisplaySats;
  const estimatedExecutionFeeSats = Math.round(
    preview.notionalSats * EXECUTION_FEE_RATE,
  );
  const estimatedGrossPayoutSats = Math.floor(
    preview.requestedContracts * SATS_PER_FULL_CONTRACT,
  );
  const estimatedProfitSats = Math.max(
    0,
    estimatedGrossPayoutSats - preview.notionalSats,
  );
  const estimatedWinFeeSats =
    state.tradeIntent === "open"
      ? Math.round(estimatedProfitSats * WIN_FEE_RATE)
      : 0;
  const estimatedFeesSats = estimatedExecutionFeeSats + estimatedWinFeeSats;
  const estimatedNetIfCorrectSats = Math.max(
    0,
    estimatedGrossPayoutSats - estimatedFeesSats,
  );
  return `
    <aside class="rounded-[21px] border border-slate-800 bg-slate-900/80 p-[21px]">
      <p class="panel-subtitle">Contract Action Ticket</p>
      <p class="mb-3 mt-1 text-sm text-slate-300">Simple by default. Market trades use a quote confirmation. Advanced covenant actions are below.</p>
      <div class="mb-3 flex items-center justify-between gap-3 border-b border-slate-800 pb-3">
        <div class="flex items-center gap-4">
          <button data-trade-intent="open" class="border-b-2 pb-1 text-xl font-medium ${state.tradeIntent === "open" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Buy</button>
          <button data-trade-intent="close" class="border-b-2 pb-1 text-xl font-medium ${state.tradeIntent === "close" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Sell</button>
        </div>
        ${
          !isBuy
            ? `<div class="flex items-center gap-2 rounded-lg border border-slate-700 p-1">
          <button data-order-type="market" class="rounded px-3 py-1 text-sm ${state.orderType === "market" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Market</button>
          <button data-order-type="limit" class="rounded px-3 py-1 text-sm ${state.orderType === "limit" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Limit</button>
        </div>`
            : ""
        }
      </div>
      <div class="mb-3 grid grid-cols-2 gap-2">
        <button data-side="yes" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "yes" ? (isBuy ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-400 bg-slate-400/15 text-slate-200") : "border-slate-700 text-slate-300"}">Yes ${yesDisplaySats} sats</button>
        <button data-side="no" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "no" ? (isBuy ? "border-rose-400 bg-rose-400/20 text-rose-200" : "border-slate-400 bg-slate-400/15 text-slate-200") : "border-slate-700 text-slate-300"}">No ${noDisplaySats} sats</button>
      </div>
      ${
        isBuy
          ? `<label for="trade-size-sats" class="mb-1 block text-xs text-slate-400">Spend amount (sats)</label>
      <input id="trade-size-sats" type="text" inputmode="numeric" value="${escapeAttr(state.tradeSizeSatsDraft)}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
      <button data-action="request-trade-quote" class="w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950">Review Buy Quote</button>
      <button data-action="toggle-buy-limit-composer" class="mt-2 w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300">${state.buyLimitComposerOpen ? "Hide Limit Buy" : "Place Limit Buy"}</button>
      ${
        state.buyLimitComposerOpen
          ? `<div class="mt-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3">
        <p class="mb-2 text-xs text-slate-500">Advanced limit buy</p>
        <label for="trade-size-contracts" class="mb-1 block text-xs text-slate-400">Contracts</label>
        <input id="trade-size-contracts" type="text" inputmode="numeric" pattern="[0-9]*" value="${escapeAttr(state.tradeContractsDraft)}" class="mb-1 h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <p class="mb-2 text-xs text-slate-500">Contracts are whole lots.</p>
        <label for="limit-price" class="mb-1 block text-xs text-slate-400">Limit price (sats)</label>
        <input id="limit-price" type="text" inputmode="numeric" pattern="[0-9]*" maxlength="2" value="${escapeAttr(state.limitPriceDraft)}" class="mb-2 h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <p class="mb-2 text-xs text-slate-500">May not fill immediately; unfilled size rests on book. ${fillabilityLabel}.</p>
        <button data-action="submit-limit-buy" class="w-full rounded-lg bg-slate-200 px-4 py-2 text-sm font-semibold text-slate-950">Place Limit Buy</button>
      </div>`
          : ""
      }`
          : `<label for="trade-size-contracts" class="mb-1 block text-xs text-slate-400">Sell amount (contracts)</label>
      <div class="mb-3 grid grid-cols-[42px_1fr_42px] gap-2">
        <button data-action="step-trade-contracts" data-contracts-step-delta="-1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Decrease contracts">&minus;</button>
        <input id="trade-size-contracts" type="text" inputmode="numeric" pattern="[0-9]*" value="${escapeAttr(state.tradeContractsDraft)}" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <button data-action="step-trade-contracts" data-contracts-step-delta="1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Increase contracts">+</button>
      </div>
      <p class="mb-3 text-xs text-slate-500">Contracts are whole lots.</p>
      ${
        !hasSellBalance
          ? `<p class="mb-3 rounded border border-slate-700 bg-slate-900/50 px-2 py-1 text-xs text-slate-300">No contracts available on this side.</p>`
          : !hasSellInput
            ? `<p class="mb-3 rounded border border-slate-700 bg-slate-900/50 px-2 py-1 text-xs text-slate-300">Enter contracts to sell.</p>`
            : ""
      }
      <div class="mb-3 flex items-center gap-2 text-sm">
        <button data-action="sell-25" class="rounded border border-slate-700 px-3 py-1 text-slate-300">25%</button>
        <button data-action="sell-50" class="rounded border border-slate-700 px-3 py-1 text-slate-300">50%</button>
        <button data-action="sell-max" class="rounded border border-slate-700 px-3 py-1 text-slate-300">Max</button>
      </div>
      ${
        state.orderType === "limit"
          ? `<label for="limit-price" class="mb-1 block text-xs text-slate-400">Limit price (sats)</label>
      <div class="mb-3 grid grid-cols-[42px_1fr_42px] gap-2">
        <button data-action="step-limit-price" data-limit-price-delta="-1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Decrease limit price">&minus;</button>
        <input id="limit-price" type="text" inputmode="numeric" pattern="[0-9]*" maxlength="2" value="${escapeAttr(state.limitPriceDraft)}" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <button data-action="step-limit-price" data-limit-price-delta="1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Increase limit price">+</button>
      </div>
      ${
        limitSellWarning
          ? `<div class="mb-3 rounded-lg border border-amber-700/60 bg-amber-950/20 p-2 text-xs text-amber-200">
        <p>Limit is ${limitSellWarning.discountSats} sats below current executable bid from SDK quote (${limitSellWarning.referencePriceSats.toFixed(2)} sats, ${limitSellWarning.discountPct.toFixed(1)}%).</p>
        ${
          state.limitSellOverrideAccepted
            ? `<p class="mt-1 text-emerald-300">Override acknowledged.</p>`
            : `<button data-action="ack-limit-sell-warning" class="mt-2 rounded border border-amber-700/70 px-2 py-1 text-[11px] text-amber-200 transition hover:bg-amber-900/30">Acknowledge and allow this price</button>`
        }
      </div>`
          : ""
      }
      ${
        limitSellWarningInfo
          ? `<div class="mb-3 rounded-lg border border-slate-700 bg-slate-900/60 p-2 text-xs text-slate-300">${escapeHtml(limitSellWarningInfo)}</div>`
          : ""
      }
      <button data-action="submit-limit-sell" class="w-full rounded-lg bg-slate-200 px-4 py-2 font-semibold text-slate-950 ${hasSellInput && !state.limitSellGuardChecking ? "" : "opacity-60"}" ${hasSellInput && !state.limitSellGuardChecking ? "" : "disabled"}>${state.limitSellGuardChecking ? "Checking reference quote..." : "Place Limit Sell"}</button>`
          : `<button data-action="request-trade-quote" class="w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950 ${hasSellInput ? "" : "opacity-60"}" ${hasSellInput ? "" : "disabled"}>Review Sell Quote</button>`
      }`
      }
      <div class="mt-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
        ${
          isBuy
            ? `<div class="flex items-center justify-between py-1"><span>You pay</span><span>${formatSats(preview.notionalSats)}</span></div>
        <div class="flex items-center justify-between py-1"><span>If correct (est.)</span><span>${formatSats(estimatedNetIfCorrectSats)}</span></div>`
            : `<div class="flex items-center justify-between py-1"><span>You receive (est.)</span><span>${formatSats(Math.max(0, preview.notionalSats - estimatedExecutionFeeSats))}</span></div>
        <div class="flex items-center justify-between py-1"><span>Position remaining (est.)</span><span>${Math.max(0, selectedPositionContracts - preview.requestedContracts).toFixed(2)} contracts</span></div>`
        }
        <div class="flex items-center justify-between py-1"><span>Estimated fees</span><span>${formatSats(estimatedFeesSats)}</span></div>
        <div class="mt-1 flex items-center justify-between py-1 text-xs text-slate-500"><span>Price</span><span>${executionPriceSats} sats · Yes + No = ${SATS_PER_FULL_CONTRACT}</span></div>
      </div>
      <div class="mt-3 flex items-center justify-between text-xs text-slate-400">
        <span>You hold: YES ${positions.yes.toFixed(2)} · NO ${positions.no.toFixed(2)}</span>
        <div class="flex items-center gap-2 rounded-lg border border-slate-700 p-1">
          ${
            !isBuy
              ? `<button data-action="sell-max" class="rounded border border-slate-700 px-2 py-1 text-slate-300">Sell max</button>`
              : ""
          }
        </div>
      </div>
      ${
        !isBuy &&
        preview.requestedContracts > selectedPositionContracts + 0.0001
          ? `<p class="mt-2 text-xs text-rose-300">Requested size exceeds your current ${state.selectedSide.toUpperCase()} position.</p>`
          : ""
      }
      <div class="mt-3 flex items-center gap-2">
        <button data-action="toggle-orderbook" class="rounded border border-slate-700 px-3 py-1.5 text-xs text-slate-300">${state.showOrderbook ? "Hide depth" : "Show depth"}</button>
        <button data-action="toggle-fee-details" class="rounded border border-slate-700 px-3 py-1.5 text-xs text-slate-300">${state.showFeeDetails ? "Hide fee details" : "Fee details"}</button>
      </div>
      ${
        state.showOrderbook
          ? `<div class="mt-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-xs">
        <p class="mb-2 font-semibold text-slate-200">${isBuy ? "Asks (buy depth)" : "Bids (sell depth)"} · ${state.selectedSide.toUpperCase()}</p>
        <div class="space-y-1">
          ${getOrderbookLevels(market, state.selectedSide, state.tradeIntent)
            .map(
              (
                level,
                idx,
              ) => `<div class="flex items-center justify-between rounded ${idx === 0 ? "bg-slate-900/70" : ""} px-2 py-1">
            <span>${level.priceSats} sats</span>
            <span>${level.contracts.toFixed(2)} contracts</span>
          </div>`,
            )
            .join("")}
        </div>
      </div>`
          : ""
      }
      ${
        state.showFeeDetails
          ? `<div class="mt-3 rounded border border-slate-800 bg-slate-900/40 p-2 text-xs text-slate-400">
        <p>Execution fee: 1% of matched notional.</p>
        <p>Winning PnL fee: 2% of positive payout minus entry cost (buy only).</p>
        <p>Final fee depends on actual matched fills.</p>
      </div>`
          : ""
      }
      <section class="mt-3 rounded-xl border border-slate-800 bg-slate-950/50 p-3">
        <div class="mb-2 flex items-center justify-between">
          <p class="text-xs text-slate-500">Live limit orders · ${state.selectedSide.toUpperCase()}</p>
          <span class="text-xs text-slate-400">${sideOrders.length}</span>
        </div>
        ${
          sideOrders.length === 0
            ? `<p class="text-xs text-slate-500">No discovered orders for this side yet.</p>`
            : `<div class="space-y-1 text-xs">
          ${sideOrders
            .slice(0, 12)
            .map((order) => {
              const availableContracts = getAvailableOrderContracts(order);
              const levelLabel =
                order.direction === "sell-base" ? "Ask" : "Bid";
              const isLocalOnly = order.source === "recovered-local";
              const canCancel = order.is_recoverable_by_current_wallet === true;
              const canFill = canRenderFillButton(order);
              return `<div class="rounded border border-slate-800 bg-slate-900/50 px-2 py-1.5">
                <div class="flex items-center justify-between">
                  <span class="text-slate-300">${levelLabel}${isLocalOnly ? ' <span class="rounded border border-amber-700/60 px-1 py-0.5 text-[10px] text-amber-300">Local only</span>' : ""}</span>
                  <div class="flex items-center gap-2">
                    <span class="text-slate-200">${order.price.toLocaleString()} sats</span>
                    ${
                      canFill
                        ? `<button data-action="fill-limit-order" data-order-id="${escapeAttr(order.id)}" class="rounded border border-slate-700 px-1.5 py-0.5 text-[10px] text-slate-300 transition hover:border-slate-500 hover:text-slate-100">Fill</button>`
                        : ""
                    }
                    ${
                      canCancel
                        ? `<button data-action="cancel-limit-order" data-order-id="${escapeAttr(order.id)}" class="rounded border border-rose-700 px-1.5 py-0.5 text-[10px] text-rose-300 transition hover:border-rose-500 hover:text-rose-200">Cancel</button>`
                        : ""
                    }
                  </div>
                </div>
                <div class="mt-0.5 flex items-center justify-between text-[10px] text-slate-500">
                  <span>${availableContracts.toLocaleString()} contracts</span>
                  <span class="mono">${escapeHtml(order.maker_base_pubkey.slice(0, 8))}...${escapeHtml(order.maker_base_pubkey.slice(-6))}</span>
                </div>
              </div>`;
            })
            .join("")}
          ${
            sideOrders.length > 12
              ? `<p class="pt-1 text-[10px] text-slate-500">+${sideOrders.length - 12} more orders</p>`
              : ""
          }
        </div>`
        }
      </section>
      <section class="mt-4 rounded-xl border border-slate-800 bg-slate-950/50 p-3">
        <div class="flex items-center justify-between">
          <p class="text-xs text-slate-500">Advanced actions</p>
          <button data-action="toggle-advanced-actions" class="rounded border border-slate-700 px-2 py-1 text-xs text-slate-300">${state.showAdvancedActions ? "Hide" : "Show"}</button>
        </div>
      </section>
      ${
        state.showAdvancedActions
          ? `
      <div class="mt-3 grid grid-cols-3 gap-2">
        <button data-tab="issue" class="rounded border px-3 py-2 text-sm ${state.actionTab === "issue" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">Issue</button>
        <button data-tab="redeem" class="rounded border px-3 py-2 text-sm ${state.actionTab === "redeem" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">Redeem</button>
        <button data-tab="cancel" class="rounded border px-3 py-2 text-sm ${state.actionTab === "cancel" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">Cancel</button>
      </div>
      ${
        state.actionTab === "issue"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.initialIssue ? "0 \u2192 1 Initial Issuance" : "1 \u2192 1 Subsequent Issuance"}</p>
        <label for="pairs-input" class="mb-1 block text-xs text-slate-400">Pairs to mint</label>
        <input id="pairs-input" type="number" min="1" step="1" value="${escapeAttr(state.pairsInput)}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Required collateral</span><span>${formatSats(issueCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: pairs * 2 * CPT (${state.pairsInput} * 2 * ${market.cptSats})</div>
        </div>
        <button data-action="submit-issue" ${paths.issue || paths.initialIssue ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.issue || paths.initialIssue ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit Issuance Transaction</button>
      </div>
      `
          : ""
      }

      ${
        state.actionTab === "redeem"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.redeem ? "Post-resolution redemption" : paths.expiryRedeem ? (market.state === 1 && expired ? "Auto finalize (1 → 4) then expiry redemption (4 → 4)" : "Expiry redemption (4 → 4)") : "Unavailable"}</p>
        <label for="tokens-input" class="mb-1 block text-xs text-slate-400">Tokens to burn</label>
        <input id="tokens-input" type="number" min="1" step="1" value="${escapeAttr(state.tokensInput)}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Collateral withdrawn</span><span>${formatSats(redeemCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: tokens * ${paths.redeem ? "2*CPT" : paths.expiryRedeem ? "CPT" : "N/A"}</div>
          ${
            market.state === 1 && expired && paths.expiryRedeem
              ? `<div class="mt-1 text-xs text-amber-300">Auto-finalize mode: this redeem action may broadcast two transactions and pay fees twice.</div>`
              : ""
          }
        </div>
        <button data-action="submit-redeem" ${paths.redeem || paths.expiryRedeem ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.redeem || paths.expiryRedeem ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit redemption tx</button>
      </div>
      `
          : ""
      }

      ${
        state.actionTab === "cancel"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: 1 \u2192 1 Cancellation</p>
        <label for="pairs-input" class="mb-1 block text-xs text-slate-400">Matched YES/NO pairs to burn</label>
        <input id="pairs-input" type="number" min="1" step="1" value="${escapeAttr(state.pairsInput)}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Collateral refund</span><span>${formatSats(cancelCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: pairs * 2 * CPT</div>
        </div>
        <button data-action="submit-cancel" ${paths.cancel ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.cancel ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit cancellation tx</button>
      </div>
      `
          : ""
      }
      `
          : ""
      }
    </aside>
    ${renderTradeQuoteModal()}
  `;
}

function renderTradeQuoteModal(): string {
  if (!state.tradeQuoteModalOpen) return "";
  const quote = state.tradeQuoteData;
  const isBuyQuote = quote?.direction === "buy";
  const nowUnix = state.tradeQuoteNowUnix;
  const expiresAtText = quote
    ? new Date(quote.expires_at_unix * 1000).toLocaleTimeString()
    : "";
  const remainingSeconds = quote
    ? getQuoteRemainingSeconds(quote.expires_at_unix, nowUnix)
    : 0;
  const effectivePriceSatsPerContract = quote
    ? getQuoteEffectivePriceSatsPerContract(quote)
    : null;
  const effectivePriceContractsPerSat = quote
    ? getQuoteEffectivePriceContractsPerSat(quote)
    : null;
  const quoteInputLabel = isBuyQuote
    ? "Exact input (sats)"
    : "Exact input (contracts)";
  const quoteOutputLabel = isBuyQuote
    ? "Expected output (contracts)"
    : "Expected output (sats)";
  const quoteInputDisplay = quote
    ? isBuyQuote
      ? formatSats(quote.total_input)
      : `${quote.total_input.toLocaleString()} contracts`
    : "";
  const quoteOutputDisplay = quote
    ? isBuyQuote
      ? `${quote.total_output.toLocaleString()} contracts`
      : formatSats(quote.total_output)
    : "";

  return `<div data-action="trade-quote-backdrop" class="fixed inset-0 z-50 flex items-center justify-center bg-black/65 backdrop-blur-sm">
    <div class="relative mx-4 w-full max-w-lg rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
      <div class="flex items-center justify-between border-b border-slate-800 px-6 py-4">
        <div>
          <h3 class="text-lg font-medium text-slate-100">Trade Quote Confirmation</h3>
          <p class="text-xs text-slate-400">Review route, pricing, and expiry before broadcast.</p>
        </div>
        <button data-action="close-trade-quote" class="rounded-lg p-2 text-slate-400 hover:bg-slate-800 hover:text-slate-200">
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </div>
      <div class="space-y-3 p-6">
        ${
          state.tradeQuoteLoading
            ? `<p class="text-sm text-slate-300">Fetching best route quote...</p>`
            : ""
        }
        ${
          state.tradeQuoteError
            ? `<p class="rounded border border-rose-700/60 bg-rose-950/20 px-3 py-2 text-sm text-rose-200">${escapeHtml(state.tradeQuoteError)}</p>`
            : ""
        }
        ${
          quote
            ? `<div class="rounded-xl border border-slate-800 bg-slate-900/50 p-3 text-sm">
          <div class="flex items-center justify-between py-1"><span>Direction</span><span>${escapeHtml(quote.direction.toUpperCase())} ${escapeHtml(quote.side.toUpperCase())}</span></div>
          <div class="flex items-center justify-between py-1"><span>${quoteInputLabel}</span><span>${quoteInputDisplay}</span></div>
          <div class="flex items-center justify-between py-1"><span>${quoteOutputLabel}</span><span>${quoteOutputDisplay}</span></div>
          <div class="flex items-center justify-between py-1"><span>Effective price</span><span>${effectivePriceSatsPerContract === null ? "N/A" : `${effectivePriceSatsPerContract.toFixed(2)} sats / contract`}</span></div>
          ${
            effectivePriceContractsPerSat === null
              ? ""
              : `<div class="mt-1 flex items-center justify-between py-1 text-xs text-slate-500"><span>Inverse</span><span>${effectivePriceContractsPerSat.toFixed(6)} contracts / sat</span></div>`
          }
          <div class="mt-1 flex items-center justify-between py-1 text-xs text-slate-500"><span>Expires</span><span>${escapeHtml(expiresAtText)} (${remainingSeconds}s)</span></div>
        </div>
        <div class="rounded-xl border border-slate-800 bg-slate-950/60 p-3 text-xs">
          <p class="mb-2 font-semibold text-slate-200">Route breakdown</p>
          ${
            quote.legs.length === 0
              ? `<p class="text-slate-500">No route legs available.</p>`
              : quote.legs
                  .map((leg) => {
                    const sourceLabel =
                      leg.source.kind === "amm_pool"
                        ? `AMM pool ${leg.source.pool_id.slice(0, 8)}...`
                        : `Limit order ${leg.source.order_id.slice(0, 8)}... @ ${leg.source.price} sats (${leg.source.lots} lots)`;
                    const legFlow = isBuyQuote
                      ? `${formatSats(leg.input_amount)} → ${leg.output_amount.toLocaleString()} contracts`
                      : `${leg.input_amount.toLocaleString()} contracts → ${formatSats(leg.output_amount)}`;
                    return `<div class="mb-1 rounded border border-slate-800 bg-slate-900/60 px-2 py-1.5">
                    <div class="flex items-center justify-between"><span>${escapeHtml(sourceLabel)}</span><span>${legFlow}</span></div>
                  </div>`;
                  })
                  .join("")
          }
        </div>`
            : ""
        }
      </div>
      <div class="flex items-center justify-end gap-2 border-t border-slate-800 px-6 py-4">
        <button data-action="close-trade-quote" class="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Cancel</button>
        <button data-action="confirm-trade-quote" class="rounded-lg bg-emerald-300 px-4 py-2 text-sm font-semibold text-slate-950 ${state.tradeQuoteLoading || state.tradeQuoteExecuting || !quote ? "opacity-60" : ""}" ${state.tradeQuoteLoading || state.tradeQuoteExecuting || !quote ? "disabled" : ""}>${state.tradeQuoteExecuting ? "Executing..." : "Confirm Trade"}</button>
      </div>
    </div>
  </div>`;
}

export function renderDetail(): string {
  const market = getSelectedMarket();
  const noPrice = market.yesPrice != null ? 1 - market.yesPrice : null;
  const paths = getPathAvailability(market);
  const expired = isExpired(market);
  const estimatedSettlementDate = getEstimatedSettlementDate(market);
  const collateralPoolSats = market.collateralUtxos.reduce(
    (sum, utxo) => sum + utxo.amountSats,
    0,
  );

  return `
    <div class="phi-container py-6 lg:py-8">
      ${
        expired && market.state === 1
          ? `<div class="mb-4 rounded-xl border border-slate-600 bg-slate-900/60 px-4 py-3 text-sm text-slate-300">Market expired unresolved at height ${market.expiryHeight}. Redeem will auto-finalize to EXPIRED first, then execute expiry redemption (can be two transactions and two fees).</div>`
          : ""
      }
      <div class="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="space-y-[21px]">
          <div class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] lg:p-[34px]">
            <button data-action="go-home" class="mb-3 flex items-center gap-1 text-sm text-slate-400 transition hover:text-slate-200">
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
              Markets
            </button>
            <div class="mb-3 flex flex-wrap items-center gap-2">
              <span class="rounded-full bg-slate-800 px-2.5 py-0.5 text-xs text-slate-300">${escapeHtml(market.category)}</span>
              ${stateBadge(market.state)}
              ${market.creationTxid ? `<button data-action="refresh-market-state" class="rounded p-0.5 text-slate-500 transition hover:text-slate-300" title="Refresh state"><svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg></button>` : ""}
              <span class="h-3.5 w-px bg-slate-700"></span>
              <button data-action="open-nostr-event" data-market-id="${escapeAttr(market.id)}" data-nevent="${escapeAttr(market.nevent)}" class="text-xs text-slate-400 transition hover:text-slate-200">Nostr Event</button>
              ${market.creationTxid ? `<button data-action="open-explorer-tx" data-txid="${escapeAttr(market.creationTxid)}" class="text-xs text-slate-400 transition hover:text-slate-200">Creation TX</button>` : ""}
            </div>
            <h1 class="phi-title mb-2 text-2xl font-medium leading-tight text-slate-100 lg:text-[34px]">${escapeHtml(market.question)}</h1>
            <p class="mb-3 text-base text-slate-400">${escapeHtml(market.description)}</p>

            <div class="mb-4 grid gap-3 sm:grid-cols-3">
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Yes price<br/><span class="text-lg font-medium text-emerald-400">${market.yesPrice != null ? formatProbabilityWithPercent(market.yesPrice) : "\u2014"}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">No price<br/><span class="text-lg font-medium text-rose-400">${noPrice != null ? formatProbabilityWithPercent(noPrice) : "\u2014"}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Settlement deadline<br/><span class="text-slate-100">Est. by ${formatSettlementDateTime(estimatedSettlementDate)}</span></div>
            </div>

            ${(() => {
              const pos = getPositionContracts(market);
              if (pos.yes === 0 && pos.no === 0) return "";
              return `<div class="mb-4 flex items-center gap-3 rounded-xl border border-slate-700 bg-slate-900/40 px-4 py-3 text-sm">
                <span class="text-slate-400">Your position</span>
                ${pos.yes > 0 ? `<span class="rounded bg-emerald-500/20 px-2 py-0.5 font-medium text-emerald-300">YES ${pos.yes.toLocaleString()}</span>` : ""}
                ${pos.no > 0 ? `<span class="rounded bg-red-500/20 px-2 py-0.5 font-medium text-red-300">NO ${pos.no.toLocaleString()}</span>` : ""}
              </div>`;
            })()}

            ${chartSkeleton(market, "detail")}
          </div>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 px-[21px] py-3">
            <div class="flex items-center justify-between gap-4">
              <p class="text-sm text-slate-400"><span class="text-slate-200">Protocol Details</span> — oracle, covenant paths, and collateral mechanics</p>
              <button data-action="toggle-advanced-details" class="shrink-0 rounded-lg border border-slate-700 px-3 py-1.5 text-sm text-slate-200">${state.showAdvancedDetails ? "Hide" : "Show"}</button>
            </div>
          </section>

          ${
            state.showAdvancedDetails
              ? `
          <div class="grid gap-3 lg:grid-cols-2">
            <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
              <p class="panel-subtitle">Oracle</p>
              <h3 class="panel-title mb-2 text-lg">Oracle Attestation</h3>
              <div class="space-y-1 text-xs text-slate-300">
                <div class="kv-row"><span class="shrink-0">Oracle</span><button data-action="copy-to-clipboard" data-copy-value="${escapeAttr(hexToNpub(market.oraclePubkey))}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${escapeAttr(hexToNpub(market.oraclePubkey))}">${(() => {
                  const n = hexToNpub(market.oraclePubkey);
                  return `${n.slice(0, 10)}...${n.slice(-6)}`;
                })()}</button></div>
                <div class="kv-row"><span class="shrink-0">Market ID</span><button data-action="copy-to-clipboard" data-copy-value="${escapeAttr(market.marketId)}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${escapeAttr(market.marketId)}">${escapeHtml(market.marketId.slice(0, 8))}...${escapeHtml(market.marketId.slice(-8))}</button></div>
                <div class="kv-row"><span class="shrink-0">Block target</span><span class="mono">${formatBlockHeight(market.expiryHeight)}</span></div>
                <div class="kv-row"><span class="shrink-0">Current height</span><span class="mono">${formatBlockHeight(market.currentHeight)}</span></div>
                <div class="kv-row"><span class="shrink-0">Message domain</span><span class="mono text-right">SHA256(ID || outcome)</span></div>
                <div class="kv-row"><span class="shrink-0">Outcome bytes</span><span class="mono">YES=0x01, NO=0x00</span></div>
                <div class="kv-row"><span class="shrink-0">Resolve status</span><span class="${market.resolveTx?.sigVerified ? "text-emerald-300" : "text-slate-400"}">${market.resolveTx ? `Attested ${market.resolveTx.outcome.toUpperCase()} @ ${market.resolveTx.height}` : "Unresolved"}</span></div>
                ${market.resolveTx ? `<div class="kv-row"><span class="shrink-0">Sig hash</span><button data-action="copy-to-clipboard" data-copy-value="${escapeAttr(market.resolveTx.signatureHash)}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${escapeAttr(market.resolveTx.signatureHash)}">${escapeHtml(market.resolveTx.signatureHash.slice(0, 8))}...${escapeHtml(market.resolveTx.signatureHash.slice(-8))}</button></div><div class="kv-row"><span class="shrink-0">Resolve tx</span><button data-action="copy-to-clipboard" data-copy-value="${escapeAttr(market.resolveTx.txid)}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${escapeAttr(market.resolveTx.txid)}">${escapeHtml(market.resolveTx.txid.slice(0, 8))}...${escapeHtml(market.resolveTx.txid.slice(-8))}</button></div>` : ""}
              </div>
              ${
                state.nostrPubkey &&
                state.nostrPubkey === market.oraclePubkey &&
                market.state === 1 &&
                !market.resolveTx
                  ? `
              <div class="mt-3 rounded-lg border border-amber-700/60 bg-amber-950/20 p-3">
                <p class="mb-2 text-sm font-semibold text-amber-200">You are the oracle for this market</p>
                <div class="flex items-center gap-2">
                  <button data-action="oracle-attest-yes" class="rounded-lg bg-emerald-300 px-4 py-2 text-sm font-semibold text-slate-950">Resolve YES</button>
                  <button data-action="oracle-attest-no" class="rounded-lg bg-rose-400 px-4 py-2 text-sm font-semibold text-slate-950">Resolve NO</button>
                </div>
              </div>`
                  : ""
              }
              ${
                state.lastAttestationSig &&
                state.lastAttestationMarketId === market.marketId &&
                market.state === 1
                  ? `
              <div class="mt-3 rounded-lg border border-emerald-700/60 bg-emerald-950/20 p-3">
                <p class="mb-2 text-sm font-semibold text-emerald-200">Attestation published — execute on-chain resolution</p>
                <p class="mb-2 text-xs text-slate-300">Outcome: ${state.lastAttestationOutcome ? "YES" : "NO"} | Sig: ${state.lastAttestationSig.slice(0, 24)}...</p>
                <button data-action="execute-resolution" ${state.resolutionExecuting ? "disabled" : ""} class="w-full rounded-lg ${state.resolutionExecuting ? "bg-slate-700 text-slate-400" : "bg-emerald-300 text-slate-950"} px-4 py-2 text-sm font-semibold">${state.resolutionExecuting ? "Executing..." : "Execute Resolution On-Chain"}</button>
              </div>`
                  : ""
              }
            </section>

            <section class="rounded-[21px] border ${market.collateralUtxos.length === 1 ? "border-emerald-800" : "border-rose-800"} bg-slate-950/55 p-[21px]">
              <p class="panel-subtitle">Integrity</p>
              <h3 class="panel-title mb-2 text-lg">Single-UTXO Integrity</h3>
              <p class="text-sm ${market.collateralUtxos.length === 1 ? "text-emerald-300" : "text-rose-300"}">${market.collateralUtxos.length === 1 ? "OK: exactly one collateral UTXO" : "ALERT: fragmented collateral UTXO set"}</p>
              <div class="mt-2 space-y-1 text-xs text-slate-300">
                ${market.collateralUtxos
                  .map(
                    (utxo) =>
                      `<div class="kv-row"><button data-action="copy-to-clipboard" data-copy-value="${escapeAttr(`${utxo.txid}:${utxo.vout}`)}" class="mono truncate hover:text-slate-100 transition cursor-pointer" title="${escapeAttr(`${utxo.txid}:${utxo.vout}`)}">${escapeHtml(utxo.txid.slice(0, 8))}...${escapeHtml(utxo.txid.slice(-8))}:${utxo.vout}</button><span class="mono shrink-0">${formatSats(utxo.amountSats)}</span></div>`,
                  )
                  .join("")}
              </div>
              <p class="mt-2 text-xs text-slate-500">Collateral pool: ${formatSats(collateralPoolSats)} · ${stateLabel(market.state)}</p>
            </section>
          </div>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <p class="panel-subtitle">Accounting</p>
            <h3 class="panel-title mb-2 text-lg">Collateral Mechanics</h3>
            <div class="grid gap-2 md:grid-cols-2 text-sm">
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Issuance: <span class="text-slate-100">pairs * 2 * CPT</span></div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Post-resolution redeem: <span class="text-slate-100">tokens * 2 * CPT</span></div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Expiry redeem: <span class="text-slate-100">tokens * CPT</span></div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Cancellation: <span class="text-slate-100">pairs * 2 * CPT</span></div>
            </div>
          </section>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <p class="panel-subtitle">State Machine</p>
            <h3 class="panel-title mb-2 text-lg">Covenant Paths</h3>
            <div class="grid gap-2 md:grid-cols-2">
              ${renderPathCard("0 \u2192 1 Initial issuance", paths.initialIssue, "pairs * 2 * CPT", "Outputs move to state-1 address")}
              ${renderPathCard("1 \u2192 1 Subsequent issuance", paths.issue, "pairs * 2 * CPT", "Collateral UTXO reconsolidated")}
              ${renderPathCard("1 \u2192 2/3 Oracle resolve", paths.resolve, "state commit via oracle signature", "All covenant outputs move atomically")}
              ${renderPathCard("2/3 Redemption", paths.redeem, "tokens * 2 * CPT", "Winning side burns tokens")}
              ${renderPathCard("1 \u2192 4 Expire transition", market.state === 1 && expired, "check_lock_height(EXPIRY_TIME)", "Reissuance + collateral move to state-4 address")}
              ${renderPathCard("4 \u2192 4 Expiry redemption", market.state === 4, "tokens * CPT", "Remaining collateral stays in EXPIRED")}
              ${renderPathCard("1 \u2192 1 Cancellation", paths.cancel, "pairs * 2 * CPT", "Equal YES/NO burn")}
            </div>
          </section>
          `
              : ""
          }
        </section>

        ${renderActionTicket(market)}
      </div>
    </div>
  `;
}
