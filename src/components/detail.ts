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
import {
  clampContractPriceSats,
  getEstimatedSettlementDate,
  getOrderbookLevels,
  getPathAvailability,
  getPositionContracts,
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
  const preview = getTradePreview(market);
  const executionPriceSats = Math.round(preview.executionPriceSats);
  const positions = getPositionContracts(market);
  const selectedPositionContracts =
    state.selectedSide === "yes" ? positions.yes : positions.no;
  const ctaVerb = state.tradeIntent === "open" ? "Buy" : "Sell";
  const ctaTarget = state.selectedSide === "yes" ? "Yes" : "No";
  const ctaLabel = `${ctaVerb} ${ctaTarget}`;
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
      <p class="mb-3 mt-1 text-sm text-slate-300">Buy or sell with a cleaner ticket flow. Advanced covenant actions are below.</p>
      <div class="mb-3 flex items-center justify-between gap-3 border-b border-slate-800 pb-3">
        <div class="flex items-center gap-4">
          <button data-trade-intent="open" class="border-b-2 pb-1 text-xl font-medium ${state.tradeIntent === "open" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Buy</button>
          <button data-trade-intent="close" class="border-b-2 pb-1 text-xl font-medium ${state.tradeIntent === "close" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Sell</button>
        </div>
        <div class="flex items-center gap-2 rounded-lg border border-slate-700 p-1">
          <button data-order-type="market" class="rounded px-3 py-1 text-sm ${state.orderType === "market" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Market</button>
          <button data-order-type="limit" class="rounded px-3 py-1 text-sm ${state.orderType === "limit" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Limit</button>
        </div>
      </div>
      <div class="mb-3 grid grid-cols-2 gap-2">
        <button data-side="yes" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "yes" ? (state.tradeIntent === "open" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-400 bg-slate-400/15 text-slate-200") : "border-slate-700 text-slate-300"}">Yes ${yesDisplaySats} sats</button>
        <button data-side="no" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "no" ? (state.tradeIntent === "open" ? "border-rose-400 bg-rose-400/20 text-rose-200" : "border-slate-400 bg-slate-400/15 text-slate-200") : "border-slate-700 text-slate-300"}">No ${noDisplaySats} sats</button>
      </div>
      <div class="mb-3 flex items-center justify-between gap-2">
        <label class="text-xs text-slate-400">Amount</label>
        <div class="grid grid-cols-2 gap-2">
          <button data-size-mode="sats" class="rounded border px-2 py-1 text-xs ${state.sizeMode === "sats" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">sats</button>
          <button data-size-mode="contracts" class="rounded border px-2 py-1 text-xs ${state.sizeMode === "contracts" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">contracts</button>
        </div>
      </div>
      ${
        state.sizeMode === "sats"
          ? `
      <input id="trade-size-sats" type="text" inputmode="numeric" value="${state.tradeSizeSatsDraft}" class="mb-2 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
      `
          : `
      <div class="mb-3 grid grid-cols-[42px_1fr_42px] gap-2">
        <button data-action="step-trade-contracts" data-contracts-step-delta="-1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Decrease contracts">&minus;</button>
        <input id="trade-size-contracts" type="text" inputmode="decimal" value="${state.tradeContractsDraft}" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <button data-action="step-trade-contracts" data-contracts-step-delta="1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Increase contracts">+</button>
      </div>
      ${
        state.tradeIntent === "close"
          ? `<div class="mb-3 flex items-center gap-2 text-sm">
        <button data-action="sell-25" class="rounded border border-slate-700 px-3 py-1 text-slate-300">25%</button>
        <button data-action="sell-50" class="rounded border border-slate-700 px-3 py-1 text-slate-300">50%</button>
        <button data-action="sell-max" class="rounded border border-slate-700 px-3 py-1 text-slate-300">Max</button>
      </div>`
          : ""
      }
      `
      }
      ${
        state.orderType === "limit"
          ? `
      <label for="limit-price" class="mb-1 block text-xs text-slate-400">Limit price (sats)</label>
      <div class="mb-3 grid grid-cols-[42px_1fr_42px] gap-2">
        <button data-action="step-limit-price" data-limit-price-delta="-1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Decrease limit price">&minus;</button>
        <input id="limit-price" type="text" inputmode="numeric" pattern="[0-9]*" maxlength="2" value="${state.limitPriceDraft}" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <button data-action="step-limit-price" data-limit-price-delta="1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Increase limit price">+</button>
      </div>
      <p class="mb-3 text-xs text-slate-500">May not fill immediately; unfilled size rests on book. ${fillabilityLabel}. Matchable now: ${formatSats(preview.executedSats)}.</p>
      `
          : `<p class="mb-3 text-xs text-slate-500">Estimated avg fill: ${preview.fill.avgPriceSats.toFixed(1)} sats (range ${preview.fill.bestPriceSats}-${preview.fill.worstPriceSats}).</p>`
      }
      <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
        ${
          state.tradeIntent === "open"
            ? `<div class="flex items-center justify-between py-1"><span>You pay</span><span>${formatSats(preview.notionalSats)}</span></div>
        <div class="flex items-center justify-between py-1"><span>If filled & correct</span><span>${formatSats(estimatedNetIfCorrectSats)}</span></div>`
            : `<div class="flex items-center justify-between py-1"><span>You receive (if filled)</span><span>${formatSats(Math.max(0, preview.notionalSats - estimatedExecutionFeeSats))}</span></div>
        <div class="flex items-center justify-between py-1"><span>Position remaining (if filled)</span><span>${Math.max(0, selectedPositionContracts - preview.requestedContracts).toFixed(2)} contracts</span></div>`
        }
        <div class="flex items-center justify-between py-1"><span>Estimated fees</span><span>${formatSats(estimatedFeesSats)}</span></div>
        <div class="mt-1 flex items-center justify-between py-1 text-xs text-slate-500"><span>Price</span><span>${executionPriceSats} sats · Yes + No = ${SATS_PER_FULL_CONTRACT}</span></div>
      </div>
      <button data-action="submit-trade" class="mt-4 w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950">${ctaLabel}</button>
      <div class="mt-3 flex items-center justify-between text-xs text-slate-400">
        <span>You hold: YES ${positions.yes.toFixed(2)} · NO ${positions.no.toFixed(2)}</span>
        ${
          state.tradeIntent === "close"
            ? `<button data-action="sell-max" class="rounded border border-slate-700 px-2 py-1 text-slate-300">Sell max</button>`
            : ""
        }
      </div>
      ${
        state.tradeIntent === "close" &&
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
        <p class="mb-2 font-semibold text-slate-200">${state.tradeIntent === "open" ? "Asks (buy depth)" : "Bids (sell depth)"} · ${state.selectedSide.toUpperCase()}</p>
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
        <input id="pairs-input" type="number" min="1" step="1" value="${state.pairsInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
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
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.redeem ? "Post-resolution redemption" : paths.expiryRedeem ? "Expiry redemption" : "Unavailable"}</p>
        <label for="tokens-input" class="mb-1 block text-xs text-slate-400">Tokens to burn</label>
        <input id="tokens-input" type="number" min="1" step="1" value="${state.tokensInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Collateral withdrawn</span><span>${formatSats(redeemCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: tokens * ${paths.redeem ? "2*CPT" : paths.expiryRedeem ? "CPT" : "N/A"}</div>
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
        <input id="pairs-input" type="number" min="1" step="1" value="${state.pairsInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
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
  `;
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
          ? `<div class="mb-4 rounded-xl border border-slate-600 bg-slate-900/60 px-4 py-3 text-sm text-slate-300">Market expired unresolved at height ${market.expiryHeight}. Expiry redemption path is active. Issuance and oracle resolve are disabled.</div>`
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
              <span class="rounded-full bg-slate-800 px-2.5 py-0.5 text-xs text-slate-300">${market.category}</span>
              ${stateBadge(market.state)}
              ${market.creationTxid ? `<button data-action="refresh-market-state" class="rounded p-0.5 text-slate-500 transition hover:text-slate-300" title="Refresh state"><svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg></button>` : ""}
              <span class="h-3.5 w-px bg-slate-700"></span>
              <button data-action="open-nostr-event" data-market-id="${market.id}" data-nevent="${market.nevent}" class="text-xs text-slate-400 transition hover:text-slate-200">Nostr Event</button>
              ${market.creationTxid ? `<button data-action="open-explorer-tx" data-txid="${market.creationTxid}" class="text-xs text-slate-400 transition hover:text-slate-200">Creation TX</button>` : ""}
            </div>
            <h1 class="phi-title mb-2 text-2xl font-medium leading-tight text-slate-100 lg:text-[34px]">${market.question}</h1>
            <p class="mb-3 text-base text-slate-400">${market.description}</p>

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
                <div class="kv-row"><span class="shrink-0">Oracle</span><button data-action="copy-to-clipboard" data-copy-value="${hexToNpub(market.oraclePubkey)}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${hexToNpub(market.oraclePubkey)}">${(() => {
                  const n = hexToNpub(market.oraclePubkey);
                  return `${n.slice(0, 10)}...${n.slice(-6)}`;
                })()}</button></div>
                <div class="kv-row"><span class="shrink-0">Market ID</span><button data-action="copy-to-clipboard" data-copy-value="${market.marketId}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${market.marketId}">${market.marketId.slice(0, 8)}...${market.marketId.slice(-8)}</button></div>
                <div class="kv-row"><span class="shrink-0">Block target</span><span class="mono">${formatBlockHeight(market.expiryHeight)}</span></div>
                <div class="kv-row"><span class="shrink-0">Current height</span><span class="mono">${formatBlockHeight(market.currentHeight)}</span></div>
                <div class="kv-row"><span class="shrink-0">Message domain</span><span class="mono text-right">SHA256(ID || outcome)</span></div>
                <div class="kv-row"><span class="shrink-0">Outcome bytes</span><span class="mono">YES=0x01, NO=0x00</span></div>
                <div class="kv-row"><span class="shrink-0">Resolve status</span><span class="${market.resolveTx?.sigVerified ? "text-emerald-300" : "text-slate-400"}">${market.resolveTx ? `Attested ${market.resolveTx.outcome.toUpperCase()} @ ${market.resolveTx.height}` : "Unresolved"}</span></div>
                ${market.resolveTx ? `<div class="kv-row"><span class="shrink-0">Sig hash</span><button data-action="copy-to-clipboard" data-copy-value="${market.resolveTx.signatureHash}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${market.resolveTx.signatureHash}">${market.resolveTx.signatureHash.slice(0, 8)}...${market.resolveTx.signatureHash.slice(-8)}</button></div><div class="kv-row"><span class="shrink-0">Resolve tx</span><button data-action="copy-to-clipboard" data-copy-value="${market.resolveTx.txid}" class="mono truncate text-right hover:text-slate-100 transition cursor-pointer" title="${market.resolveTx.txid}">${market.resolveTx.txid.slice(0, 8)}...${market.resolveTx.txid.slice(-8)}</button></div>` : ""}
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
                      `<div class="kv-row"><button data-action="copy-to-clipboard" data-copy-value="${utxo.txid}:${utxo.vout}" class="mono truncate hover:text-slate-100 transition cursor-pointer" title="${utxo.txid}:${utxo.vout}">${utxo.txid.slice(0, 8)}...${utxo.txid.slice(-8)}:${utxo.vout}</button><span class="mono shrink-0">${formatSats(utxo.amountSats)}</span></div>`,
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
              ${renderPathCard("1 Expiry redemption", paths.expiryRedeem, "tokens * CPT", "Unresolved + expiry only")}
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
