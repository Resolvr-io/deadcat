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
import { hexToNpub } from "../../utils/crypto";
import { formatSats } from "../../utils-react/format";
import {
  getMarketById,
  getPathAvailability,
  getPositionContracts,
  isExpired,
  stateLabel,
} from "../../utils-react/market";
import { generateMockPriceHistory } from "../../utils-react/mock-price-history";
import MarketChart from "../chart/MarketChart";
import MarketHeader from "./MarketHeader";
import TradingPanel from "./TradingPanel";

export default function DetailPage() {
  const selectedMarketId = useStore((s) => s.selectedMarketId);
  const showAdvancedDetails = useStore((s) => s.showAdvancedDetails);
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
            <MarketHeader market={market} />

            <MarketChart
              market={market}
              priceHistory={priceHistory}
              mode="detail"
            />
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

          {/* Advanced details toggle */}
          <section className="rounded-[21px] border border-slate-800 bg-slate-950/55 px-[21px] py-3">
            <div className="flex items-center justify-between gap-4">
              <p className="text-sm text-slate-400">
                <span className="text-slate-200">Protocol Details</span> —
                oracle, covenant paths, and collateral mechanics
              </p>
              <button
                type="button"
                onClick={() =>
                  useStore.setState({
                    showAdvancedDetails: !showAdvancedDetails,
                  })
                }
                className="shrink-0 rounded-lg border border-slate-700 px-3 py-1.5 text-sm text-slate-200"
              >
                {showAdvancedDetails ? "Hide" : "Show"}
              </button>
            </div>
          </section>

          {/* Advanced details expanded */}
          {showAdvancedDetails && (
            <>
              <AdvancedDetails market={market} />
              <CollateralMechanics />
              <CovenantPaths market={market} />
            </>
          )}
        </section>

        {/* Right column — Trading panel */}
        <TradingPanel market={market} />
      </div>
    </div>
  );
}

// ── Advanced detail sub-components ──────────────────────────────────

function AdvancedDetails({ market }: { market: import("../../types").Market }) {
  const collateralPoolSats = market.collateralUtxos.reduce(
    (sum, utxo) => sum + utxo.amountSats,
    0,
  );

  const npub = useMemo(
    () => hexToNpub(market.oraclePubkey),
    [market.oraclePubkey],
  );

  const truncatedNpub = `${npub.slice(0, 10)}...${npub.slice(-6)}`;

  return (
    <div className="grid gap-3 lg:grid-cols-2">
      <section className="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
        <p className="panel-subtitle">Oracle</p>
        <h3 className="panel-title mb-2 text-lg">Oracle Attestation</h3>
        <div className="space-y-1 text-xs text-slate-300">
          <div className="kv-row">
            <span className="shrink-0">Oracle</span>
            <button
              type="button"
              onClick={() => navigator.clipboard.writeText(npub)}
              className="mono truncate text-right transition hover:text-slate-100"
              title={npub}
            >
              {truncatedNpub}
            </button>
          </div>
          <div className="kv-row">
            <span className="shrink-0">Market ID</span>
            <button
              type="button"
              onClick={() => navigator.clipboard.writeText(market.marketId)}
              className="mono truncate text-right transition hover:text-slate-100"
              title={market.marketId}
            >
              {market.marketId.slice(0, 8)}...{market.marketId.slice(-8)}
            </button>
          </div>
          <div className="kv-row">
            <span className="shrink-0">Block target</span>
            <span className="mono">
              {market.expiryHeight.toLocaleString("en-US")}
            </span>
          </div>
          <div className="kv-row">
            <span className="shrink-0">Current height</span>
            <span className="mono">
              {market.currentHeight.toLocaleString("en-US")}
            </span>
          </div>
          <div className="kv-row">
            <span className="shrink-0">Message domain</span>
            <span className="mono text-right">SHA256(ID || outcome)</span>
          </div>
          <div className="kv-row">
            <span className="shrink-0">Outcome bytes</span>
            <span className="mono">YES=0x01, NO=0x00</span>
          </div>
          <div className="kv-row">
            <span className="shrink-0">Resolve status</span>
            <span
              className={
                market.resolveTx?.sigVerified
                  ? "text-emerald-300"
                  : "text-slate-400"
              }
            >
              {market.resolveTx
                ? `Attested ${market.resolveTx.outcome.toUpperCase()} @ ${market.resolveTx.height}`
                : "Unresolved"}
            </span>
          </div>
          {market.resolveTx && (
            <>
              <div className="kv-row">
                <span className="shrink-0">Sig hash</span>
                <button
                  type="button"
                  onClick={() =>
                    navigator.clipboard.writeText(
                      market.resolveTx?.signatureHash ?? "",
                    )
                  }
                  className="mono truncate text-right transition hover:text-slate-100"
                  title={market.resolveTx.signatureHash}
                >
                  {market.resolveTx.signatureHash.slice(0, 8)}...
                  {market.resolveTx.signatureHash.slice(-8)}
                </button>
              </div>
              <div className="kv-row">
                <span className="shrink-0">Resolve tx</span>
                <button
                  type="button"
                  onClick={() =>
                    navigator.clipboard.writeText(market.resolveTx?.txid ?? "")
                  }
                  className="mono truncate text-right transition hover:text-slate-100"
                  title={market.resolveTx.txid}
                >
                  {market.resolveTx.txid.slice(0, 8)}...
                  {market.resolveTx.txid.slice(-8)}
                </button>
              </div>
            </>
          )}
        </div>
      </section>

      <section
        className={`rounded-[21px] border ${market.collateralUtxos.length === 1 ? "border-emerald-800" : "border-rose-800"} bg-slate-950/55 p-[21px]`}
      >
        <p className="panel-subtitle">Integrity</p>
        <h3 className="panel-title mb-2 text-lg">Single-UTXO Integrity</h3>
        <p
          className={`text-sm ${market.collateralUtxos.length === 1 ? "text-emerald-300" : "text-rose-300"}`}
        >
          {market.collateralUtxos.length === 1
            ? "OK: exactly one collateral UTXO"
            : "ALERT: fragmented collateral UTXO set"}
        </p>
        <div className="mt-2 space-y-1 text-xs text-slate-300">
          {market.collateralUtxos.map((utxo) => (
            <div key={`${utxo.txid}:${utxo.vout}`} className="kv-row">
              <button
                type="button"
                onClick={() =>
                  navigator.clipboard.writeText(`${utxo.txid}:${utxo.vout}`)
                }
                className="mono truncate transition hover:text-slate-100"
                title={`${utxo.txid}:${utxo.vout}`}
              >
                {utxo.txid.slice(0, 8)}...{utxo.txid.slice(-8)}:{utxo.vout}
              </button>
              <span className="mono shrink-0">
                {formatSats(utxo.amountSats)}
              </span>
            </div>
          ))}
        </div>
        <p className="mt-2 text-xs text-slate-500">
          Collateral pool: {formatSats(collateralPoolSats)} ·{" "}
          {stateLabel(market.state)}
        </p>
      </section>
    </div>
  );
}

function CollateralMechanics() {
  return (
    <section className="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
      <p className="panel-subtitle">Accounting</p>
      <h3 className="panel-title mb-2 text-lg">Collateral Mechanics</h3>
      <div className="grid gap-2 text-sm md:grid-cols-2">
        <div className="rounded-lg border border-slate-800 bg-slate-900/40 p-3">
          Issuance: <span className="text-slate-100">pairs * 2 * CPT</span>
        </div>
        <div className="rounded-lg border border-slate-800 bg-slate-900/40 p-3">
          Post-resolution redeem:{" "}
          <span className="text-slate-100">tokens * 2 * CPT</span>
        </div>
        <div className="rounded-lg border border-slate-800 bg-slate-900/40 p-3">
          Expiry redeem: <span className="text-slate-100">tokens * CPT</span>
        </div>
        <div className="rounded-lg border border-slate-800 bg-slate-900/40 p-3">
          Cancellation: <span className="text-slate-100">pairs * 2 * CPT</span>
        </div>
      </div>
    </section>
  );
}

function PathCard({
  label,
  enabled,
  formula,
  next,
}: {
  label: string;
  enabled: boolean;
  formula: string;
  next: string;
}) {
  return (
    <div
      className={`rounded-lg border px-3 py-2 ${
        enabled
          ? "border-slate-700 bg-slate-900/50 text-slate-300"
          : "border-slate-800 bg-slate-950/60 text-slate-400"
      }`}
    >
      <div className="mb-1 flex items-center justify-between gap-2">
        <p className="text-sm font-medium">{label}</p>
        <span
          className={`status-chip ${enabled ? "border border-emerald-500/40 bg-emerald-500/10 text-emerald-300" : "border border-slate-700 bg-slate-800/60 text-slate-400"}`}
        >
          {enabled ? "Available" : "Locked"}
        </span>
      </div>
      <p className="text-xs">{formula}</p>
      <p className="mt-1 text-xs">{next}</p>
    </div>
  );
}

function CovenantPaths({ market }: { market: import("../../types").Market }) {
  const paths = getPathAvailability(market);
  const expired = isExpired(market);

  return (
    <section className="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
      <p className="panel-subtitle">State Machine</p>
      <h3 className="panel-title mb-2 text-lg">Covenant Paths</h3>
      <div className="grid gap-2 md:grid-cols-2">
        <PathCard
          label={"\u0030 \u2192 1 Initial issuance"}
          enabled={paths.initialIssue}
          formula="pairs * 2 * CPT"
          next="Outputs move to state-1 address"
        />
        <PathCard
          label={"1 \u2192 1 Subsequent issuance"}
          enabled={paths.issue}
          formula="pairs * 2 * CPT"
          next="Collateral UTXO reconsolidated"
        />
        <PathCard
          label={"1 \u2192 2/3 Oracle resolve"}
          enabled={paths.resolve}
          formula="state commit via oracle signature"
          next="All covenant outputs move atomically"
        />
        <PathCard
          label="2/3 Redemption"
          enabled={paths.redeem}
          formula="tokens * 2 * CPT"
          next="Winning side burns tokens"
        />
        <PathCard
          label={"1 \u2192 4 Expire transition"}
          enabled={market.state === 1 && expired}
          formula="check_lock_height(EXPIRY_TIME)"
          next="Reissuance + collateral move to state-4 address"
        />
        <PathCard
          label={"4 \u2192 4 Expiry redemption"}
          enabled={market.state === 4}
          formula="tokens * CPT"
          next="Remaining collateral stays in EXPIRED"
        />
        <PathCard
          label={"1 \u2192 1 Cancellation"}
          enabled={paths.cancel}
          formula="pairs * 2 * CPT"
          next="Equal YES/NO burn"
        />
      </div>
    </section>
  );
}
