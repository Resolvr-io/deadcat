import { useCallback, useMemo } from "react";
import {
  useCancelTokens,
  useIssueTokens,
  useRedeemExpiredTokens,
  useRedeemTokens,
} from "../../queries/mutations/useMarketOps";
import {
  useCreateLimitOrder,
  useExecuteTrade,
  useQuoteTrade,
} from "../../queries/mutations/useTrading";
import { DEFAULT_TX_OPTIONS } from "../../services/tx";
import { useStore } from "../../store";
import type { ActionTab, Market, TradeDirection } from "../../types";
import { formatSats } from "../../utils-react/format";
import { friendlyError } from "../../utils-react/friendly-error";
import {
  clampContractPriceSats,
  commitLimitPriceDraft,
  commitTradeContractsDraft,
  commitTradeSizeSatsDraft,
  fullContractSats,
  getPathAvailability,
  getPositionContracts,
  getTradePreview,
  isExpired,
  setLimitPriceSats,
} from "../../utils-react/market";
import OrderbookPanel from "./OrderbookPanel";
import PoolSection from "./PoolSection";

export default function TradingPanel({ market }: { market: Market }) {
  const selectedSide = useStore((s) => s.selectedSide);
  const orderType = useStore((s) => s.orderType);
  const actionTab = useStore((s) => s.actionTab);
  const tradeIntent = useStore((s) => s.tradeIntent);
  const sizeMode = useStore((s) => s.sizeMode);
  const tradeSizeSats = useStore((s) => s.tradeSizeSats);
  const tradeSizeSatsDraft = useStore((s) => s.tradeSizeSatsDraft);
  const tradeContracts = useStore((s) => s.tradeContracts);
  const tradeContractsDraft = useStore((s) => s.tradeContractsDraft);
  const limitPrice = useStore((s) => s.limitPrice);
  const limitPriceDraft = useStore((s) => s.limitPriceDraft);
  const tradeQuoteLoading = useStore((s) => s.tradeQuoteLoading);
  const tradeExecuteLoading = useStore((s) => s.tradeExecuteLoading);
  const tradeQuoteSnapshot = useStore((s) => s.tradeQuoteSnapshot);
  const tradeError = useStore((s) => s.tradeError);
  const showOrderbook = useStore((s) => s.showOrderbook);
  const walletStatus = useStore((s) => s.walletStatus);
  const walletData = useStore((s) => s.walletData);
  const marketMakerMode = useStore((s) => s.marketMakerMode);
  const showAdvancedActions = useStore((s) => s.showAdvancedActions);
  const pairsInput = useStore((s) => s.pairsInput);
  const tokensInput = useStore((s) => s.tokensInput);
  const quoteTradeMutation = useQuoteTrade();
  const executeTradeMutation = useExecuteTrade();
  const createLimitOrderMutation = useCreateLimitOrder();
  const issueTokensMutation = useIssueTokens();
  const redeemTokensMutation = useRedeemTokens();
  const redeemExpiredMutation = useRedeemExpiredTokens();
  const cancelTokensMutation = useCancelTokens();

  const fc = fullContractSats(market);
  const paths = getPathAvailability(market);
  const positions = getPositionContracts(market, walletData);
  const selectedPositionContracts =
    selectedSide === "yes" ? positions.yes : positions.no;

  const currentDirection: TradeDirection =
    tradeIntent === "open" ? "buy" : "sell";
  const currentExactInput =
    currentDirection === "buy"
      ? Math.max(1, Math.floor(tradeSizeSats))
      : Math.max(1, Math.floor(tradeContracts));

  const preview = useMemo(
    () =>
      getTradePreview(
        market,
        {
          selectedSide,
          orderType,
          tradeIntent,
          sizeMode,
          limitPrice,
          tradeSizeSats,
          tradeContracts,
        },
        walletData,
      ),
    [
      market,
      selectedSide,
      orderType,
      tradeIntent,
      sizeMode,
      limitPrice,
      tradeSizeSats,
      tradeContracts,
      walletData,
    ],
  );

  const walletPolicyAssetId = useStore((s) => s.walletPolicyAssetId);
  const tradeBusy = tradeQuoteLoading || tradeExecuteLoading;
  const executionPriceSats = Math.round(preview.executionPriceSats);
  const ctaVerb = tradeIntent === "open" ? "Buy" : "Sell";
  const ctaTarget = selectedSide === "yes" ? "Yes" : "No";
  const ctaLabel = `${ctaVerb} ${ctaTarget}`;

  const policyBalance =
    walletData && walletPolicyAssetId
      ? (walletData.balance[walletPolicyAssetId] ?? 0)
      : 0;
  const insufficientFunds =
    walletStatus === "unlocked" &&
    tradeIntent === "open" &&
    policyBalance < tradeSizeSats;

  const ctaBg =
    insufficientFunds || tradeBusy
      ? "bg-slate-700 text-slate-400"
      : selectedSide === "yes"
        ? "bg-emerald-300 text-slate-950 hover:bg-emerald-200"
        : "bg-rose-400 text-slate-950 hover:bg-rose-300";

  const yesPrice = market.yesPrice;
  const yesDisplaySats =
    yesPrice != null
      ? clampContractPriceSats(Math.round(yesPrice * fc), fc)
      : null;
  const noDisplaySats = yesDisplaySats != null ? fc - yesDisplaySats : null;
  const yesPct = yesPrice != null ? Math.round(yesPrice * 100) : null;
  const noPct = yesPct != null ? 100 - yesPct : null;

  const estimatedGrossPayoutSats = Math.floor(preview.requestedContracts * fc);

  // Quote snapshot matching
  const snapshot = tradeQuoteSnapshot;
  const quote =
    snapshot &&
    snapshot.marketId === market.id &&
    snapshot.side === selectedSide &&
    snapshot.direction === currentDirection &&
    snapshot.exactInput === currentExactInput
      ? snapshot.quote
      : null;

  const inputUnit: "sats" | "contracts" =
    currentDirection === "buy" ? "sats" : "contracts";
  const outputUnit: "sats" | "contracts" =
    currentDirection === "buy" ? "contracts" : "sats";

  const formatTradeAmount = (
    amount: number,
    unit: "sats" | "contracts",
  ): string =>
    unit === "sats"
      ? formatSats(Math.max(0, Math.floor(amount)))
      : `${Math.max(0, amount).toLocaleString(undefined, { maximumFractionDigits: 2 })} contracts`;

  // ── Handlers ──────────────────────────────────────────────────────

  const handleSubmitTrade = useCallback(() => {
    if (orderType === "limit") {
      const priceSats = Math.round(limitPrice * fc);
      const amount = currentExactInput;
      if (amount <= 0) return;

      useStore.setState({ tradeExecuteLoading: true, tradeError: null });
      createLimitOrderMutation.mutate(
        {
          market,
          side: selectedSide,
          direction: currentDirection,
          price: priceSats,
          amount,
        },
        {
          onSuccess: () => {
            useStore.setState({ tradeExecuteLoading: false });
          },
          onError: (error) => {
            const msg = String(error);
            const tradeErr =
              msg.includes("excluding") && msg.includes("fee")
                ? "Limit orders need two L-BTC UTXOs (one for the order, one for the fee). Send yourself a small amount first to split your balance."
                : msg;
            useStore.setState({
              tradeExecuteLoading: false,
              tradeError: tradeErr,
            });
          },
        },
      );
      return;
    }

    // Market order: quote then execute
    useStore.setState({
      tradeQuoteLoading: true,
      tradeExecuteLoading: false,
      tradeError: null,
    });

    quoteTradeMutation.mutate(
      {
        market,
        side: selectedSide,
        direction: currentDirection,
        exactInput: currentExactInput,
      },
      {
        onSuccess: (quoteResult) => {
          useStore.setState({
            tradeQuoteSnapshot: {
              marketId: market.id,
              side: selectedSide,
              direction: currentDirection,
              exactInput: currentExactInput,
              quote: quoteResult,
            },
            tradeQuoteLoading: false,
            tradeExecuteLoading: true,
          });

          executeTradeMutation.mutate(
            {
              market,
              side: selectedSide,
              direction: currentDirection,
              exactInput: currentExactInput,
              txOptions: DEFAULT_TX_OPTIONS,
              expectedQuote: {
                total_input: quoteResult.total_input,
                total_output: quoteResult.total_output,
                legs: quoteResult.legs,
              },
            },
            {
              onSuccess: () => {
                useStore.setState({
                  tradeQuoteLoading: false,
                  tradeExecuteLoading: false,
                });
              },
              onError: (error) => {
                useStore.setState({
                  tradeError: String(error),
                  tradeQuoteLoading: false,
                  tradeExecuteLoading: false,
                });
              },
            },
          );
        },
        onError: (error) => {
          useStore.setState({
            tradeError: String(error),
            tradeQuoteLoading: false,
            tradeExecuteLoading: false,
          });
        },
      },
    );
  }, [
    orderType,
    limitPrice,
    fc,
    currentExactInput,
    market,
    selectedSide,
    currentDirection,
    createLimitOrderMutation,
    quoteTradeMutation,
    executeTradeMutation,
  ]);

  const handleSellPreset = useCallback(
    (fraction: number) => {
      const maxContracts = selectedPositionContracts;
      const target =
        fraction >= 1
          ? maxContracts
          : Math.max(1, Math.floor(maxContracts * fraction));
      useStore.setState({
        tradeContracts: target,
        tradeContractsDraft: String(target),
      });
    },
    [selectedPositionContracts],
  );

  const handleStepLimitPrice = useCallback(
    (delta: number) => {
      const currentSats = Math.round(limitPrice * fc);
      const result = setLimitPriceSats(market, currentSats + delta);
      useStore.setState(result);
    },
    [limitPrice, fc, market],
  );

  const handleStepContracts = useCallback(
    (delta: number) => {
      const next = Math.max(0, Math.floor(tradeContracts) + delta);
      useStore.setState({
        tradeContracts: next,
        tradeContractsDraft: String(next),
      });
    },
    [tradeContracts],
  );

  const handleSubmitIssue = useCallback(() => {
    const pairs = Math.max(1, Math.floor(pairsInput));
    issueTokensMutation.mutate({ market, pairs });
  }, [market, pairsInput, issueTokensMutation]);

  const handleSubmitRedeem = useCallback(() => {
    const tokens = Math.max(1, Math.floor(tokensInput));
    if (paths.redeem) {
      redeemTokensMutation.mutate({ market, tokens });
    } else if (paths.expiryRedeem) {
      const walletBalance = walletData?.balance;
      const yesBalance = walletBalance
        ? (walletBalance[
            market.yesAssetId.match(/.{2}/g)?.reverse().join("") ?? ""
          ] ?? 0)
        : 0;
      const tokenAssetHex =
        yesBalance > 0 ? market.yesAssetId : market.noAssetId;
      redeemExpiredMutation.mutate({ market, tokenAssetHex, tokens });
    }
  }, [
    market,
    tokensInput,
    paths,
    walletData,
    redeemTokensMutation,
    redeemExpiredMutation,
  ]);

  const handleSubmitCancel = useCallback(() => {
    const pairs = Math.max(1, Math.floor(pairsInput));
    cancelTokensMutation.mutate({ market, pairs });
  }, [market, pairsInput, cancelTokensMutation]);

  // Issue/Redeem/Cancel collateral calculations
  const issueCollateral = pairsInput * 2 * market.cptSats;
  const cancelCollateral = pairsInput * 2 * market.cptSats;
  const redeemRate = paths.redeem
    ? 2 * market.cptSats
    : paths.expiryRedeem
      ? market.cptSats
      : 0;
  const redeemCollateral = tokensInput * redeemRate;

  return (
    <aside className="rounded-[21px] border border-slate-800 bg-slate-900/80 p-[21px]">
      {/* Buy / Sell toggle */}
      <div className="mb-3 flex items-center justify-between gap-3 border-b border-slate-800 pb-3">
        <div className="flex items-center gap-4">
          <button
            type="button"
            onClick={() =>
              useStore.setState({
                tradeIntent: "open",
                sizeMode: "sats",
              })
            }
            className={`border-b-2 pb-1 text-xl font-medium ${tradeIntent === "open" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}`}
          >
            Buy
          </button>
          <button
            type="button"
            onClick={() =>
              useStore.setState({
                tradeIntent: "close",
                sizeMode: "contracts",
              })
            }
            className={`border-b-2 pb-1 text-xl font-medium ${tradeIntent === "close" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}`}
          >
            Sell
          </button>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => useStore.setState({ orderType: "market" })}
            className={`rounded border px-2 py-1 text-xs ${orderType === "market" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-400"}`}
          >
            Market
          </button>
          <button
            type="button"
            onClick={() => useStore.setState({ orderType: "limit" })}
            className={`rounded border px-2 py-1 text-xs ${orderType === "limit" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-400"}`}
          >
            Limit
          </button>
        </div>
      </div>

      {/* Side selector */}
      <div className="mb-3 grid grid-cols-2 gap-2">
        <button
          type="button"
          onClick={() => useStore.setState({ selectedSide: "yes" })}
          className={`rounded-xl border px-3 py-3 ${
            selectedSide === "yes"
              ? tradeIntent === "open"
                ? "border-emerald-500 bg-emerald-500 text-slate-950"
                : "border-slate-500 bg-slate-600 text-slate-100"
              : "border-slate-700 text-slate-300 hover:border-slate-500"
          }`}
        >
          <span className="block text-lg font-semibold">
            Yes{yesPct != null ? ` ${yesPct}%` : ""}
          </span>
          {yesDisplaySats != null && (
            <span className="block text-xs opacity-60">
              {yesDisplaySats} sats/contract
            </span>
          )}
        </button>
        <button
          type="button"
          onClick={() => useStore.setState({ selectedSide: "no" })}
          className={`rounded-xl border px-3 py-3 ${
            selectedSide === "no"
              ? tradeIntent === "open"
                ? "border-rose-500 bg-rose-500 text-slate-950"
                : "border-slate-500 bg-slate-600 text-slate-100"
              : "border-slate-700 text-slate-300 hover:border-slate-500"
          }`}
        >
          <span className="block text-lg font-semibold">
            No{noPct != null ? ` ${noPct}%` : ""}
          </span>
          {noDisplaySats != null && (
            <span className="block text-xs opacity-60">
              {noDisplaySats} sats/contract
            </span>
          )}
        </button>
      </div>

      {/* Limit price input */}
      {orderType === "limit" && (
        <div className="mb-3">
          <label
            className="mb-1 block text-xs text-slate-400"
            htmlFor="limit-price-input"
          >
            Price (sats per contract)
          </label>
          <div className="grid grid-cols-[42px_1fr_42px] gap-2">
            <button
              type="button"
              onClick={() => handleStepLimitPrice(-1)}
              className="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800"
              aria-label="Decrease price"
            >
              &minus;
            </button>
            <input
              id="limit-price-input"
              type="text"
              inputMode="numeric"
              value={limitPriceDraft}
              onChange={(e) =>
                useStore.setState({ limitPriceDraft: e.target.value })
              }
              onBlur={() => {
                const result = commitLimitPriceDraft(
                  limitPriceDraft,
                  market,
                  limitPrice,
                );
                useStore.setState(result);
              }}
              className="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2"
            />
            <button
              type="button"
              onClick={() => handleStepLimitPrice(1)}
              className="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800"
              aria-label="Increase price"
            >
              +
            </button>
          </div>
        </div>
      )}

      {/* Size mode toggle */}
      <div className="mb-3 flex items-center justify-between gap-2">
        <span className="text-xs text-slate-400">Amount</span>
        <div className="grid grid-cols-2 gap-2">
          <button
            type="button"
            disabled={tradeIntent === "close"}
            onClick={() => useStore.setState({ sizeMode: "sats" })}
            className={`rounded border px-2 py-1 text-xs ${sizeMode === "sats" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"} ${tradeIntent === "close" ? "cursor-not-allowed opacity-50" : ""}`}
          >
            sats
          </button>
          <button
            type="button"
            disabled={tradeIntent === "open"}
            onClick={() => useStore.setState({ sizeMode: "contracts" })}
            className={`rounded border px-2 py-1 text-xs ${sizeMode === "contracts" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"} ${tradeIntent === "open" ? "cursor-not-allowed opacity-50" : ""}`}
          >
            contracts
          </button>
        </div>
      </div>

      {/* Size input */}
      {tradeIntent === "open" ? (
        <>
          <div className="relative mb-2">
            <input
              type="text"
              inputMode="numeric"
              value={tradeSizeSatsDraft}
              onChange={(e) =>
                useStore.setState({ tradeSizeSatsDraft: e.target.value })
              }
              onBlur={() => {
                const result = commitTradeSizeSatsDraft(tradeSizeSatsDraft);
                useStore.setState(result);
              }}
              className="h-12 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 pr-14 text-sm"
            />
            <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-slate-500">
              sats
            </span>
          </div>
          {tradeSizeSats > 0 && (
            <div className="mb-3 flex items-center justify-between rounded-lg bg-slate-900/60 px-3 py-2 text-sm">
              <span className="text-slate-400">Potential payout</span>
              <span className="font-semibold text-emerald-300">
                {formatSats(estimatedGrossPayoutSats)}
                {preview.notionalSats > 0 &&
                  estimatedGrossPayoutSats > preview.notionalSats && (
                    <span className="ml-1.5 text-xs text-slate-500">
                      {(
                        estimatedGrossPayoutSats / preview.notionalSats
                      ).toFixed(2)}
                      x
                    </span>
                  )}
              </span>
            </div>
          )}
        </>
      ) : (
        <>
          <div className="mb-3 grid grid-cols-[42px_1fr_42px] gap-2">
            <button
              type="button"
              onClick={() => handleStepContracts(-1)}
              className="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800"
              aria-label="Decrease contracts"
            >
              &minus;
            </button>
            <input
              type="text"
              inputMode="decimal"
              value={tradeContractsDraft}
              onChange={(e) =>
                useStore.setState({ tradeContractsDraft: e.target.value })
              }
              onBlur={() => {
                const result = commitTradeContractsDraft(
                  tradeContractsDraft,
                  market,
                  selectedSide,
                  tradeIntent,
                  walletData,
                );
                useStore.setState(result);
              }}
              className="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2"
            />
            <button
              type="button"
              onClick={() => handleStepContracts(1)}
              className="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800"
              aria-label="Increase contracts"
            >
              +
            </button>
          </div>
          {tradeIntent === "close" && (
            <div className="mb-3 flex items-center gap-2 text-sm">
              <button
                type="button"
                onClick={() => handleSellPreset(0.25)}
                className="rounded border border-slate-700 px-3 py-1 text-slate-300"
              >
                25%
              </button>
              <button
                type="button"
                onClick={() => handleSellPreset(0.5)}
                className="rounded border border-slate-700 px-3 py-1 text-slate-300"
              >
                50%
              </button>
              <button
                type="button"
                onClick={() => handleSellPreset(1)}
                className="rounded border border-slate-700 px-3 py-1 text-slate-300"
              >
                Max
              </button>
            </div>
          )}
        </>
      )}

      {/* Preview / Quote section */}
      {orderType === "limit" ? (
        <>
          <div className="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
            <div className="flex items-center justify-between py-1">
              <span>Order type</span>
              <span>Limit</span>
            </div>
            <div className="flex items-center justify-between py-1">
              <span>Price</span>
              <span>{Math.round(limitPrice * fc)} sats</span>
            </div>
            <div className="flex items-center justify-between py-1">
              <span>Amount</span>
              <span>
                {currentDirection === "buy"
                  ? formatSats(Math.max(1, Math.floor(tradeSizeSats)))
                  : `${Math.max(1, Math.floor(tradeContracts))} contracts`}
              </span>
            </div>
            <div className="mt-1 flex items-center justify-between py-1 text-xs text-slate-500">
              <span>Side</span>
              <span>
                {selectedSide.toUpperCase()} · {currentDirection}
              </span>
            </div>
          </div>
          {tradeError && (
            <p className="mt-3 text-xs text-rose-300">
              {friendlyError(tradeError)}
            </p>
          )}
          {market.traderCount > 0 && (
            <p className="mt-4 text-center text-xs text-slate-500">
              {market.traderCount.toLocaleString()} traders in this market
            </p>
          )}
          <button
            type="button"
            onClick={handleSubmitTrade}
            disabled={tradeBusy || insufficientFunds}
            className={`mt-3 w-full rounded-lg ${ctaBg} px-4 py-2 font-semibold transition`}
          >
            {tradeExecuteLoading
              ? "Placing..."
              : insufficientFunds
                ? "Insufficient funds"
                : `Place Limit ${ctaLabel}`}
          </button>
        </>
      ) : (
        <>
          {preview.fill.filledContracts > 0 && (
            <p className="mb-3 text-xs text-slate-500">
              Avg fill: {preview.fill.avgPriceSats.toFixed(1)} sats (range{" "}
              {preview.fill.bestPriceSats}–{preview.fill.worstPriceSats})
            </p>
          )}
          <div className="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
            {tradeIntent === "open" ? (
              <>
                <div className="flex items-center justify-between py-1">
                  <span>Amount</span>
                  <span>{formatSats(preview.notionalSats)}</span>
                </div>
                <div className="flex items-center justify-between py-1">
                  <span>Max payout</span>
                  <span>{formatSats(estimatedGrossPayoutSats)}</span>
                </div>
              </>
            ) : (
              <>
                <div className="flex items-center justify-between py-1">
                  <span>You receive</span>
                  <span>{formatSats(preview.notionalSats)}</span>
                </div>
                <div className="flex items-center justify-between py-1">
                  <span>Position after</span>
                  <span>
                    {Math.max(
                      0,
                      selectedPositionContracts - preview.requestedContracts,
                    ).toFixed(2)}{" "}
                    contracts
                  </span>
                </div>
              </>
            )}
            <div className="mt-1 flex items-center justify-between py-1 text-xs text-slate-500">
              <span>Avg price</span>
              <span>
                {executionPriceSats} sats · Yes + No = {fc}
              </span>
            </div>
          </div>

          {tradeQuoteLoading && (
            <p className="mt-3 text-xs text-slate-400">
              Requesting live LMSR quote...
            </p>
          )}

          {quote && (
            <div className="mt-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-xs">
              <p className="mb-2 font-semibold text-slate-200">
                Live route quote
              </p>
              <div className="mb-2 flex items-center justify-between text-slate-400">
                <span>Total input</span>
                <span>{formatTradeAmount(quote.total_input, inputUnit)}</span>
              </div>
              <div className="mb-2 flex items-center justify-between text-slate-400">
                <span>Total output</span>
                <span>{formatTradeAmount(quote.total_output, outputUnit)}</span>
              </div>
              <div className="mb-2 flex items-center justify-between text-slate-500">
                <span>Effective price</span>
                <span>{quote.effective_price.toFixed(6)}</span>
              </div>
              <div className="space-y-1">
                {quote.legs.map((leg, idx) => {
                  const sourceLabel =
                    leg.source.kind === "lmsr_pool"
                      ? `LMSR ${leg.source.pool_id.slice(0, 10)}... s:${leg.source.old_s_index}->${leg.source.new_s_index}`
                      : `Maker ${leg.source.order_id.slice(0, 10)}... @ ${leg.source.price}`;
                  const legKey =
                    leg.source.kind === "lmsr_pool"
                      ? `lmsr-${leg.source.pool_id}-${leg.source.old_s_index}-${leg.source.new_s_index}`
                      : `maker-${leg.source.order_id}-${leg.source.price}`;
                  return (
                    <div
                      key={legKey}
                      className="flex items-center justify-between rounded border border-slate-800 bg-slate-900/50 px-2 py-1"
                    >
                      <span className="text-slate-300">
                        {idx + 1}. {sourceLabel}
                      </span>
                      <span className="text-slate-400">
                        {formatTradeAmount(leg.input_amount, inputUnit)} -&gt;{" "}
                        {formatTradeAmount(leg.output_amount, outputUnit)}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {!quote && !tradeQuoteLoading && (
            <p className="mt-3 text-xs text-slate-500">
              No live route quote yet for current trade inputs.
            </p>
          )}

          {tradeError && (
            <p className="mt-3 text-xs text-rose-300">
              {friendlyError(tradeError)}
            </p>
          )}

          {market.traderCount > 0 && (
            <p className="mb-2 mt-4 text-center text-xs text-slate-500">
              {market.traderCount.toLocaleString()} traders in this market
            </p>
          )}
          {walletStatus === "not_created" ? (
            <div
              className={`${market.traderCount > 0 ? "" : "mt-4"} rounded-lg border border-slate-700 bg-slate-900/60 p-4 text-center`}
            >
              <p className="text-sm font-medium text-slate-300">
                Create an account to trade
              </p>
              <p className="mt-1 text-xs text-slate-500">
                Your quote will be preserved
              </p>
              <button
                type="button"
                onClick={() =>
                  useStore.setState({
                    setupModalOpen: true,
                    onboardingStep: "nostr",
                  })
                }
                className="mt-3 w-full rounded-lg bg-emerald-400 px-4 py-2.5 font-semibold text-slate-950 transition hover:bg-emerald-300"
              >
                Set up account
              </button>
            </div>
          ) : walletStatus === "locked" ? (
            <button
              type="button"
              onClick={() => useStore.setState({ walletOpen: true })}
              className={`${market.traderCount > 0 ? "" : "mt-4"} flex w-full items-center justify-center gap-2 rounded-lg bg-slate-700 px-4 py-2 font-semibold text-slate-300 transition hover:bg-slate-600`}
            >
              <svg
                aria-hidden="true"
                className="-mt-px h-4 w-4 shrink-0"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                <path d="M7 11V7a5 5 0 0110 0v4" />
              </svg>
              Unlock wallet to trade
            </button>
          ) : (
            <button
              type="button"
              onClick={handleSubmitTrade}
              disabled={tradeBusy || insufficientFunds}
              className={`${market.traderCount > 0 ? "" : "mt-4"} w-full rounded-lg ${ctaBg} px-4 py-2 font-semibold transition`}
            >
              {tradeExecuteLoading
                ? "Executing..."
                : tradeQuoteLoading
                  ? "Quoting..."
                  : insufficientFunds
                    ? "Insufficient funds"
                    : ctaLabel}
            </button>
          )}
        </>
      )}

      {/* Position info */}
      <div className="mt-3 flex items-center justify-between text-xs text-slate-400">
        <span>
          You hold: YES {positions.yes.toFixed(2)} · NO{" "}
          {positions.no.toFixed(2)}
        </span>
        {tradeIntent === "close" && (
          <button
            type="button"
            onClick={() => handleSellPreset(1)}
            className="rounded border border-slate-700 px-2 py-1 text-slate-300"
          >
            Sell max
          </button>
        )}
      </div>

      {/* Size exceeds position warning */}
      {tradeIntent === "close" &&
        preview.requestedContracts > selectedPositionContracts + 0.0001 && (
          <p className="mt-2 text-xs text-rose-300">
            Requested size exceeds your current {selectedSide.toUpperCase()}{" "}
            position.
          </p>
        )}

      {/* Fee details */}
      <details className="mt-3 text-xs text-slate-500">
        <summary className="cursor-pointer select-none hover:text-slate-300">
          Fee details
        </summary>
        <div className="mt-2 space-y-1 rounded border border-slate-800 bg-slate-900/40 p-2">
          <p>Execution fee: 1% of matched notional.</p>
          <p>
            Winning PnL fee: 2% of positive payout minus entry cost (buy only).
          </p>
          <p>Final fee depends on actual matched fills.</p>
        </div>
      </details>

      {/* Collapsible order book */}
      <div className="mt-4 border-t border-slate-800 pt-3">
        <button
          type="button"
          onClick={() => useStore.setState({ showOrderbook: !showOrderbook })}
          className="flex w-full items-center justify-between text-xs font-semibold uppercase tracking-wider text-slate-400 transition hover:text-slate-200"
        >
          <span>Order Book</span>
          <span className="font-normal normal-case">
            {showOrderbook ? "Hide" : "Show"}
          </span>
        </button>
        {showOrderbook && <OrderbookPanel market={market} />}
      </div>

      {/* Market maker pool & advanced actions sections */}
      {marketMakerMode && <PoolSection market={market} />}

      {marketMakerMode && showAdvancedActions && (
        <AdvancedActionsPanel
          market={market}
          actionTab={actionTab}
          paths={paths}
          pairsInput={pairsInput}
          tokensInput={tokensInput}
          issueCollateral={issueCollateral}
          redeemCollateral={redeemCollateral}
          cancelCollateral={cancelCollateral}
          onSubmitIssue={handleSubmitIssue}
          onSubmitRedeem={handleSubmitRedeem}
          onSubmitCancel={handleSubmitCancel}
        />
      )}
    </aside>
  );
}

// ── Advanced Actions (Issue / Redeem / Cancel) ──────────────────────

function AdvancedActionsPanel({
  market,
  actionTab,
  paths,
  pairsInput,
  tokensInput,
  issueCollateral,
  redeemCollateral,
  cancelCollateral,
  onSubmitIssue,
  onSubmitRedeem,
  onSubmitCancel,
}: {
  market: Market;
  actionTab: ActionTab;
  paths: import("../../types").PathAvailability;
  pairsInput: number;
  tokensInput: number;
  issueCollateral: number;
  redeemCollateral: number;
  cancelCollateral: number;
  onSubmitIssue: () => void;
  onSubmitRedeem: () => void;
  onSubmitCancel: () => void;
}) {
  const expired = isExpired(market);

  return (
    <>
      <div className="mt-3 grid grid-cols-3 gap-2">
        {(["issue", "redeem", "cancel"] as const).map((tab) => (
          <button
            type="button"
            key={tab}
            onClick={() => useStore.setState({ actionTab: tab })}
            className={`rounded border px-3 py-2 text-sm ${actionTab === tab ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}`}
          >
            {tab.charAt(0).toUpperCase() + tab.slice(1)}
          </button>
        ))}
      </div>

      {actionTab === "issue" && (
        <div className="mt-3">
          <p className="mb-2 text-sm text-slate-300">
            Path:{" "}
            {paths.initialIssue
              ? "0 \u2192 1 Initial Issuance"
              : "1 \u2192 1 Subsequent Issuance"}
          </p>
          <label
            className="mb-1 block text-xs text-slate-400"
            htmlFor="pairs-to-mint-input"
          >
            Pairs to mint
          </label>
          <input
            id="pairs-to-mint-input"
            type="number"
            min="1"
            step="1"
            value={pairsInput}
            onChange={(e) =>
              useStore.setState({ pairsInput: Number(e.target.value) })
            }
            className="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm"
          />
          <div className="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
            <div className="flex items-center justify-between">
              <span>Required collateral</span>
              <span>{formatSats(issueCollateral)}</span>
            </div>
            <div className="mt-1 text-xs text-slate-400">
              Formula: pairs * 2 * CPT ({pairsInput} * 2 * {market.cptSats})
            </div>
          </div>
          <button
            type="button"
            onClick={onSubmitIssue}
            disabled={!paths.issue && !paths.initialIssue}
            className={`mt-4 w-full rounded-lg ${paths.issue || paths.initialIssue ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold`}
          >
            Submit Issuance Transaction
          </button>
        </div>
      )}

      {actionTab === "redeem" && (
        <div className="mt-3">
          <p className="mb-2 text-sm text-slate-300">
            Path:{" "}
            {paths.redeem
              ? "Post-resolution redemption"
              : paths.expiryRedeem
                ? market.state === 1 && expired
                  ? "Auto finalize (1 \u2192 4) then expiry redemption (4 \u2192 4)"
                  : "Expiry redemption (4 \u2192 4)"
                : "Unavailable"}
          </p>
          <label
            className="mb-1 block text-xs text-slate-400"
            htmlFor="tokens-to-burn-input"
          >
            Tokens to burn
          </label>
          <input
            id="tokens-to-burn-input"
            type="number"
            min="1"
            step="1"
            value={tokensInput}
            onChange={(e) =>
              useStore.setState({ tokensInput: Number(e.target.value) })
            }
            className="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm"
          />
          <div className="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
            <div className="flex items-center justify-between">
              <span>Collateral withdrawn</span>
              <span>{formatSats(redeemCollateral)}</span>
            </div>
            <div className="mt-1 text-xs text-slate-400">
              Formula: tokens *{" "}
              {paths.redeem ? "2*CPT" : paths.expiryRedeem ? "CPT" : "N/A"}
            </div>
            {market.state === 1 && expired && paths.expiryRedeem && (
              <div className="mt-1 text-xs text-amber-300">
                Auto-finalize mode: this redeem action may broadcast two
                transactions and pay fees twice.
              </div>
            )}
          </div>
          <button
            type="button"
            onClick={onSubmitRedeem}
            disabled={!paths.redeem && !paths.expiryRedeem}
            className={`mt-4 w-full rounded-lg ${paths.redeem || paths.expiryRedeem ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold`}
          >
            Submit redemption tx
          </button>
        </div>
      )}

      {actionTab === "cancel" && (
        <div className="mt-3">
          <p className="mb-2 text-sm text-slate-300">
            Path: 1 {"\u2192"} 1 Cancellation
          </p>
          <label
            className="mb-1 block text-xs text-slate-400"
            htmlFor="pairs-to-cancel-input"
          >
            Matched YES/NO pairs to burn
          </label>
          <input
            id="pairs-to-cancel-input"
            type="number"
            min="1"
            step="1"
            value={pairsInput}
            onChange={(e) =>
              useStore.setState({ pairsInput: Number(e.target.value) })
            }
            className="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm"
          />
          <div className="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
            <div className="flex items-center justify-between">
              <span>Collateral refund</span>
              <span>{formatSats(cancelCollateral)}</span>
            </div>
            <div className="mt-1 text-xs text-slate-400">
              Formula: pairs * 2 * CPT
            </div>
          </div>
          <button
            type="button"
            onClick={onSubmitCancel}
            disabled={!paths.cancel}
            className={`mt-4 w-full rounded-lg ${paths.cancel ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold`}
          >
            Submit cancellation tx
          </button>
        </div>
      )}
    </>
  );
}
