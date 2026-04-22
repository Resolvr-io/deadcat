import { invoke } from "@tauri-apps/api/core";
import { useCallback, useMemo } from "react";
import {
  useRedeemExpiredTokens,
  useRedeemTokens,
} from "../../queries/mutations/useMarketOps";
import { useMarketOrders, useMarkets } from "../../queries/useMarkets";
import { usePriceHistory } from "../../queries/usePools";
import { useStore } from "../../store";
import type { AttestationResult } from "../../types";
import { formatSats } from "../../utils-react/format";
import {
  getMarketById,
  getPathAvailability,
  getPositionContracts,
  isExpired,
} from "../../utils-react/market";
import { generateMockPriceHistory } from "../../utils-react/mock-price-history";
import MarketChart from "../chart/MarketChart";
import { MarketHeaderBottom, MarketHeaderTop } from "./MarketHeader";
import TradingPanel from "./TradingPanel";

export default function DetailPage() {
  const selectedMarketId = useStore((s) => s.selectedMarketId);
  const marketMakerMode = useStore((s) => s.marketMakerMode);
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const walletData = useStore((s) => s.walletData);
  const attestationLoading = useStore((s) => s.attestationLoading);

  const redeemMutation = useRedeemTokens();
  const redeemExpiredMutation = useRedeemExpiredTokens();

  const { data: markets = [] } = useMarkets();
  const market = useMemo(
    () => getMarketById(selectedMarketId, markets),
    [selectedMarketId, markets],
  );

  useMarketOrders(market?.marketId ?? null);

  const { data: rawPriceHistory = [] } = usePriceHistory(
    market?.marketId ?? null,
  );
  const priceHistory =
    rawPriceHistory.length > 0 && market
      ? rawPriceHistory
      : market
        ? generateMockPriceHistory(market)
        : [];

  const handleOracleResolve = useCallback(
    async (outcomeYes: boolean) => {
      if (!market?.anchor) return;
      useStore.setState({ attestationLoading: true });
      try {
        const attestation = await invoke<AttestationResult>("oracle_attest", {
          marketIdHex: market.marketId,
          outcomeYes,
        });
        await invoke("resolve_market", {
          contractParamsJson: JSON.stringify(market.anchor),
          anchor: market.anchor,
          outcomeYes,
          oracleSignatureHex: attestation.signature_hex,
          txOptions: {
            feePolicy: { kind: "confirmation_target_blocks", blocks: 2 },
          },
        });
      } catch (e) {
        console.error("Oracle resolve failed:", e);
      } finally {
        useStore.setState({ attestationLoading: false });
      }
    },
    [market],
  );

  if (!market) {
    return (
      <div className="phi-container py-16 text-center">
        <p className="text-slate-400">Market not found.</p>
      </div>
    );
  }

  const paths = getPathAvailability(market);
  const expired = isExpired(market);
  const positions = getPositionContracts(market, walletData);

  const isResolved = market.state === 2 || market.state === 3;
  const canExpRedeem = paths.expiryRedeem;
  const winningSide =
    market.state === 2 ? "yes" : market.state === 3 ? "no" : null;
  const winningTokens =
    winningSide === "yes"
      ? positions.yes
      : winningSide === "no"
        ? positions.no
        : 0;
  const expiryTokens = canExpRedeem ? positions.yes + positions.no : 0;
  const redeemableTokens = isResolved ? winningTokens : expiryTokens;
  const payoutPerToken = isResolved ? 2 * market.cptSats : market.cptSats;
  const estimatedPayout = redeemableTokens * payoutPerToken;

  const isOracle =
    marketMakerMode &&
    nostrPubkey != null &&
    nostrPubkey === market.oraclePubkey &&
    market.state === 1 &&
    !market.resolveTx;

  return (
    <div className="phi-container py-6 lg:py-8">
      {expired && market.state === 1 && (
        <div className="mb-4 rounded-xl border border-slate-600 bg-slate-900/60 px-4 py-3 text-sm text-slate-300">
          Market expired unresolved at height {market.expiryHeight}. Redeem will
          auto-finalize to EXPIRED first, then execute expiry redemption (can be
          two transactions and two fees).
        </div>
      )}

      <div className="grid gap-[21px] lg:grid-cols-[1.618fr_1fr]">
        {/* Left column */}
        <section className="space-y-[21px]">
          <div className="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] lg:p-[34px]">
            <MarketHeaderTop market={market} />

            <MarketChart
              market={market}
              priceHistory={priceHistory}
              mode="detail"
            />

            <div className="mt-5">
              <MarketHeaderBottom market={market} />
            </div>
          </div>

          {/* Redemption banner */}
          {(isResolved || canExpRedeem) && redeemableTokens > 0 && (
            <section className="rounded-[21px] border border-emerald-700/60 bg-emerald-950/20 p-[21px]">
              <p className="mb-2 text-sm font-semibold text-emerald-200">
                {isResolved
                  ? `Market resolved ${winningSide?.toUpperCase()}`
                  : "Market expired"}{" "}
                — you have redeemable tokens
              </p>
              <p className="mb-3 text-xs text-slate-400">
                Estimated payout: {formatSats(estimatedPayout)}
              </p>
              <button
                type="button"
                onClick={() => {
                  if (isResolved) {
                    redeemMutation.mutate({ market, tokens: redeemableTokens });
                  } else {
                    redeemExpiredMutation.mutate({
                      market,
                      tokenAssetHex: market.yesAssetId,
                      tokens: redeemableTokens,
                    });
                  }
                }}
                disabled={
                  redeemMutation.isPending || redeemExpiredMutation.isPending
                }
                className="w-full rounded-lg bg-emerald-300 px-4 py-2 text-sm font-semibold text-slate-950"
              >
                {isResolved
                  ? `Redeem ${redeemableTokens} winning ${winningSide?.toUpperCase()} token${redeemableTokens !== 1 ? "s" : ""}`
                  : `Redeem ${redeemableTokens} expired token${redeemableTokens !== 1 ? "s" : ""}`}
              </button>
            </section>
          )}

          {/* Oracle resolution panel */}
          {isOracle && (
            <section className="rounded-[21px] border border-amber-700/60 bg-amber-950/20 p-[21px]">
              <p className="mb-2 text-sm font-semibold text-amber-200">
                You are the oracle for this market
              </p>
              <p className="mb-3 text-xs text-slate-400">
                Publish an attestation and execute the on-chain resolution in
                one step.
              </p>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  disabled={attestationLoading}
                  onClick={() => void handleOracleResolve(true)}
                  className={`rounded-lg ${attestationLoading ? "bg-slate-600 text-slate-400" : "bg-emerald-300 text-slate-950"} px-4 py-2 text-sm font-semibold`}
                >
                  {attestationLoading ? "Attesting..." : "Resolve YES"}
                </button>
                <button
                  type="button"
                  disabled={attestationLoading}
                  onClick={() => void handleOracleResolve(false)}
                  className={`rounded-lg ${attestationLoading ? "bg-slate-600 text-slate-400" : "bg-rose-400 text-slate-950"} px-4 py-2 text-sm font-semibold`}
                >
                  {attestationLoading ? "Attesting..." : "Resolve NO"}
                </button>
              </div>
            </section>
          )}
        </section>

        {/* Right column — Trading panel */}
        <TradingPanel market={market} />
      </div>
    </div>
  );
}
