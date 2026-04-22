import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useMemo, useState } from "react";
import { useWalletNeedsBackup } from "../../hooks/useWalletNeedsBackup";
import {
  useLockWallet,
  useSyncWallet,
} from "../../queries/mutations/useWalletOps";
import { useMarkets } from "../../queries/useMarkets";
import { useStore } from "../../store";
import type { PaymentSwap } from "../../types";
import { reverseHex } from "../../utils/crypto";
import { satsToFiatStr } from "../../utils-react/format";
import { btcLabel, formatLbtc, satsLabel } from "../../utils-react/wallet";
import { WalletModal } from "../modals/WalletModal";
import { HiddenBalance, PawIcon } from "../shared/PawIcon";
import { ActivityList } from "./ActivityList";
import { UtxoList } from "./UtxoList";
import { WalletBackupDialog } from "./WalletBackupDialog";

const PAGE_SIZE = 10;

type WalletAssetLabel = {
  side: string;
  question: string;
  marketId: string;
};

export function WalletUnlocked({
  networkBadge,
}: {
  networkBadge: React.ReactNode;
}) {
  const walletData = useStore((s) => s.walletData);
  const walletPolicyAssetId = useStore((s) => s.walletPolicyAssetId);
  const walletUnit = useStore((s) => s.walletUnit);
  const walletBalanceHidden = useStore((s) => s.walletBalanceHidden);
  const walletError = useStore((s) => s.walletError);
  const walletLoading = useStore((s) => s.walletLoading);
  const walletTokenPage = useStore((s) => s.walletTokenPage);
  const walletNetwork = useStore((s) => s.walletNetwork);
  const showLbtcLabel = useStore((s) => s.showLbtcLabel);
  const baseCurrency = useStore((s) => s.baseCurrency);
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const myPools = useStore((s) => s.myPools);
  const cancellingOrderId = useStore((s) => s.cancellingOrderId);
  const walletModal = useStore((s) => s.walletModal);

  const { data: markets = [] } = useMarkets();
  const needsBackup = useWalletNeedsBackup();
  const [backupDialogOpen, setBackupDialogOpen] = useState(false);

  const syncWallet = useSyncWallet();
  const lockWallet = useLockWallet();

  const loading = walletLoading || syncWallet.isPending;

  const policyBalance =
    walletData && walletPolicyAssetId
      ? (walletData.balance[walletPolicyAssetId] ?? 0)
      : 0;

  const hasUnconfirmed = useMemo(
    () => (walletData?.transactions ?? []).some((tx) => tx.height == null),
    [walletData?.transactions],
  );

  // Map asset IDs to labels
  const assetLabel = useMemo(() => {
    const map = new Map<string, WalletAssetLabel>();
    for (const m of markets) {
      if (m.yesAssetId)
        map.set(reverseHex(m.yesAssetId), {
          side: "YES",
          question: m.question,
          marketId: m.id,
        });
      if (m.noAssetId)
        map.set(reverseHex(m.noAssetId), {
          side: "NO",
          question: m.question,
          marketId: m.id,
        });
      if (m.yesReissuanceToken)
        map.set(reverseHex(m.yesReissuanceToken), {
          side: "YES RT",
          question: m.question,
          marketId: m.id,
        });
      if (m.noReissuanceToken)
        map.set(reverseHex(m.noReissuanceToken), {
          side: "NO RT",
          question: m.question,
          marketId: m.id,
        });
    }
    return map;
  }, [markets]);

  // Token positions
  const tokenPositions = useMemo(() => {
    return Object.entries(walletData?.balance ?? {})
      .filter(([id, amt]) => id !== walletPolicyAssetId && amt > 0)
      .map(([id, amt]) => ({
        assetId: id,
        amount: amt,
        info: assetLabel.get(id),
      }))
      .sort((a, b) => a.assetId.localeCompare(b.assetId));
  }, [walletData?.balance, walletPolicyAssetId, assetLabel]);

  // Limit orders for user
  const myLimitOrders = useMemo(() => {
    const result: Array<{
      order: (typeof markets)[0]["limitOrders"][0];
      market: (typeof markets)[0];
    }> = [];
    for (const m of markets) {
      for (const o of m.limitOrders) {
        if (nostrPubkey && o.creator_pubkey === nostrPubkey) {
          result.push({ order: o, market: m });
        }
      }
    }
    return result;
  }, [markets, nostrPubkey]);

  const handleSync = useCallback(() => {
    useStore.setState({ walletLoading: true, walletError: "" });
    syncWallet.mutate(undefined, {
      onSuccess: async () => {
        // sync_wallet triggers backend wallet_snapshot events asynchronously.
        // Also refresh swaps (not covered by wallet_snapshot).
        try {
          const swaps = await invoke<PaymentSwap[]>("list_payment_swaps");
          const wd = useStore.getState().walletData;
          if (wd) {
            useStore.setState({ walletData: { ...wd, swaps } });
          }
        } catch {
          // swaps refresh failed — non-critical
        }
        // Wait for wallet_snapshot events to arrive before clearing spinner
        await new Promise((r) => setTimeout(r, 1000));
        useStore.setState({ walletLoading: false });
      },
      onError: (e) => {
        useStore.setState({ walletLoading: false, walletError: String(e) });
      },
    });
  }, [syncWallet]);

  const handleLock = useCallback(() => {
    lockWallet.mutate(undefined, {
      onSuccess: () => {
        useStore.setState({
          walletData: null,
          walletPassword: "",
          walletModal: "none",
          walletStatus: "locked",
        });
      },
      onError: (e) => {
        useStore.setState({ walletError: String(e) });
      },
    });
  }, [lockWallet]);

  const handleOpenReceive = useCallback(() => {
    useStore.setState({
      walletModal: "receive",
      walletModalTab: "lightning",
      receiveAmount: "",
      receiveCreating: false,
      receiveError: "",
      receiveLightningSwap: null,
      receiveBitcoinSwap: null,
      modalQr: "",
    });
  }, []);

  const handleOpenSend = useCallback(() => {
    useStore.setState({
      walletModal: "send",
      walletModalTab: "lightning",
      sendInvoice: "",
      sendLiquidAddress: "",
      sendLiquidAmount: "",
      sendBtcAmount: "",
      sendCreating: false,
      sendError: "",
      sentLightningSwap: null,
      sentLiquidResult: null,
      sentBitcoinSwap: null,
      modalQr: "",
    });
  }, []);

  const handleToggleBalanceHidden = useCallback(() => {
    useStore.setState((s) => ({ walletBalanceHidden: !s.walletBalanceHidden }));
  }, []);

  const handleSetUnit = useCallback((unit: "sats" | "btc") => {
    useStore.setState({ walletUnit: unit });
  }, []);

  const handleOpenExplorerAsset = useCallback(
    (assetId: string) => {
      const base =
        walletNetwork === "testnet"
          ? "https://blockstream.info/liquidtestnet"
          : "https://blockstream.info/liquid";
      void openUrl(`${base}/asset/${assetId}`);
    },
    [walletNetwork],
  );

  const handleOpenMarket = useCallback((marketId: string) => {
    useStore.setState({
      selectedMarketId: marketId,
      view: "detail",
      walletOpen: false,
    });
  }, []);

  const handleCancelOrder = useCallback((orderId: string) => {
    useStore.setState({ cancellingOrderId: orderId });
    // Cancel logic handled via store/mutations
  }, []);

  const errorHtml = walletError ? (
    <div className="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">
      {walletError}
    </div>
  ) : null;

  const tokenPageItems = tokenPositions.slice(
    walletTokenPage * PAGE_SIZE,
    (walletTokenPage + 1) * PAGE_SIZE,
  );
  const tokenTotalPages = Math.max(
    1,
    Math.ceil(tokenPositions.length / PAGE_SIZE),
  );

  // Known assets for display
  const KNOWN_ASSETS: Record<string, string> = {
    "38fca2d939696061a8f76d4e6b5eecd54e3b4221c846f24a6b279e79952850a5": "TEST",
  };

  return (
    <div className="px-6 py-6">
      <div className="space-y-6">
        {errorHtml}

        {/* Backup reminder — not dismissible, clears when user confirms
            they've saved the mnemonic in WalletBackupDialog. */}
        {needsBackup && (
          <button
            type="button"
            onClick={() => setBackupDialogOpen(true)}
            className="flex w-full items-start gap-3 rounded-lg border border-amber-500/40 bg-amber-500/10 p-4 text-left transition hover:bg-amber-500/15"
          >
            <svg
              aria-hidden="true"
              className="mt-0.5 h-5 w-5 shrink-0 text-amber-300"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
              />
            </svg>
            <div className="flex-1">
              <p className="text-sm font-semibold text-amber-200">
                Back up your recovery phrase
              </p>
              <p className="mt-0.5 text-xs text-amber-200/70">
                Save your 12 words somewhere safe. Without them you can't
                recover this wallet if you lose your device. Click to view and
                confirm.
              </p>
            </div>
          </button>
        )}

        <WalletBackupDialog
          open={backupDialogOpen}
          onClose={() => setBackupDialogOpen(false)}
        />

        {/* Balance */}
        <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-6 text-center relative">
          <div className="absolute left-4 top-4">{networkBadge}</div>
          <div className="absolute right-4 top-4 flex gap-1.5">
            <button
              type="button"
              onClick={handleSync}
              className="flex h-7 w-7 items-center justify-center rounded-md text-slate-500 hover:bg-slate-800 hover:text-slate-300 transition"
              disabled={loading}
              title="Sync wallet"
            >
              <svg
                aria-hidden="true"
                className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`}
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth="2"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                />
              </svg>
            </button>
            <button
              type="button"
              onClick={handleLock}
              className="flex h-7 w-7 items-center justify-center rounded-md text-slate-500 hover:bg-slate-800 hover:text-slate-300 transition"
              title="Lock wallet"
            >
              <svg
                aria-hidden="true"
                className="h-3.5 w-3.5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth="2"
              >
                <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                <path d="M7 11V7a5 5 0 0110 0v4" />
              </svg>
            </button>
          </div>
          <div className="flex items-center justify-center gap-2 text-sm text-slate-400">
            <span>Balance</span>
            <button
              type="button"
              onClick={handleToggleBalanceHidden}
              className="text-slate-500 hover:text-slate-300"
              title={walletBalanceHidden ? "Show balance" : "Hide balance"}
            >
              {walletBalanceHidden ? (
                <svg
                  aria-hidden="true"
                  className="h-4 w-4"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="M2 12C5 7 8.5 5 12 5s7 2 10 7c-3 5-6.5 7-10 7S5 17 2 12z" />
                  <ellipse cx="12" cy="12" rx="2" ry="3.5" />
                  <line x1="2" y1="2" x2="22" y2="22" />
                </svg>
              ) : (
                <svg
                  aria-hidden="true"
                  className="h-4 w-4"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="M2 12C5 7 8.5 5 12 5s7 2 10 7c-3 5-6.5 7-10 7S5 17 2 12z" />
                  <ellipse cx="12" cy="12" rx="2" ry="3.5" />
                </svg>
              )}
            </button>
          </div>
          <div className="mt-1 font-medium tracking-tight text-slate-100 h-[2.5rem] flex items-center justify-center">
            {walletBalanceHidden ? (
              <HiddenBalance className="text-3xl text-slate-500" />
            ) : (
              <span
                className={`text-3xl ${hasUnconfirmed ? "animate-pulse" : ""}`}
              >
                {formatLbtc(policyBalance, walletUnit, showLbtcLabel)}
              </span>
            )}
          </div>
          {!walletBalanceHidden && baseCurrency !== "BTC" && (
            <div className="text-sm text-slate-400 h-5 flex items-center justify-center">
              {satsToFiatStr(policyBalance, baseCurrency)}
            </div>
          )}
          <div className="mt-2 flex items-center justify-center gap-1 rounded-full border border-slate-700 mx-auto w-fit text-xs">
            <button
              type="button"
              onClick={() => handleSetUnit("sats")}
              className={`rounded-full px-3 py-1 transition ${walletUnit === "sats" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}`}
            >
              {satsLabel(showLbtcLabel)}
            </button>
            <button
              type="button"
              onClick={() => handleSetUnit("btc")}
              className={`rounded-full px-3 py-1 transition ${walletUnit === "btc" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}`}
            >
              {btcLabel(showLbtcLabel)}
            </button>
          </div>
        </div>

        {/* Action Buttons */}
        <div className="grid grid-cols-2 gap-4">
          <button
            type="button"
            onClick={handleOpenReceive}
            className="flex items-center justify-center gap-3 rounded-xl border border-emerald-400/30 bg-emerald-900/20 px-6 py-4 font-semibold text-emerald-300 transition hover:bg-emerald-900/40"
          >
            <svg
              aria-hidden="true"
              xmlns="http://www.w3.org/2000/svg"
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <line x1="12" y1="5" x2="12" y2="19" />
              <polyline points="19 12 12 19 5 12" />
            </svg>
            Receive
          </button>
          <button
            type="button"
            onClick={handleOpenSend}
            className="flex items-center justify-center gap-3 rounded-xl border border-slate-600 bg-slate-800/60 px-6 py-4 font-medium text-slate-200 transition hover:bg-slate-800"
          >
            <svg
              aria-hidden="true"
              xmlns="http://www.w3.org/2000/svg"
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <line x1="12" y1="19" x2="12" y2="5" />
              <polyline points="5 12 12 5 19 12" />
            </svg>
            Send
          </button>
        </div>

        {/* No Positions */}
        {tokenPositions.length === 0 && (
          <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-6 text-center">
            <p className="text-sm text-slate-400">No token positions yet</p>
            <p className="mt-1 text-xs text-slate-500">
              Issue tokens on a market to start trading
            </p>
          </div>
        )}

        {/* Token Positions */}
        {tokenPositions.length > 0 && (
          <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
            <h3 className="mb-3 font-semibold text-slate-100">
              Token Positions
            </h3>
            {tokenPageItems.map((tp) => {
              const shortAsset = `${tp.assetId.slice(0, 8)}...${tp.assetId.slice(-4)}`;
              if (tp.info) {
                const sideColor =
                  tp.info.side === "YES" ? "text-emerald-300" : "text-red-300";
                const sideBg =
                  tp.info.side === "YES"
                    ? "bg-emerald-500/20"
                    : "bg-red-500/20";
                const truncQ =
                  tp.info.question.length > 50
                    ? `${tp.info.question.slice(0, 50)}...`
                    : tp.info.question;
                return (
                  <div
                    key={tp.assetId}
                    className="flex items-center justify-between border-b border-slate-800 py-3 text-sm"
                  >
                    <div className="flex items-center gap-2">
                      <span
                        className={`rounded ${sideBg} px-1.5 py-0.5 text-[10px] font-medium ${sideColor}`}
                      >
                        {tp.info.side}
                      </span>
                      <button
                        type="button"
                        onClick={() =>
                          handleOpenMarket(tp.info?.marketId ?? "")
                        }
                        className="text-slate-300 hover:text-slate-100 transition cursor-pointer text-left"
                      >
                        {truncQ}
                      </button>
                    </div>
                    {walletBalanceHidden ? (
                      <span className="inline-flex items-center h-5 gap-0.5 text-slate-500">
                        <PawIcon />
                        <PawIcon />
                      </span>
                    ) : (
                      <span className="inline-flex items-center h-5 mono text-slate-100">
                        {tp.amount.toLocaleString()}
                      </span>
                    )}
                  </div>
                );
              }
              const knownName = KNOWN_ASSETS[tp.assetId];
              const label = knownName ?? shortAsset;
              const badge = knownName ?? "TOKEN";
              return (
                <div
                  key={tp.assetId}
                  className="flex items-center justify-between border-b border-slate-800 py-3 text-sm"
                >
                  <div className="flex items-center gap-2">
                    <span className="rounded bg-slate-600/30 px-1.5 py-0.5 text-[10px] font-medium text-slate-400">
                      {badge}
                    </span>
                    <button
                      type="button"
                      onClick={() => handleOpenExplorerAsset(tp.assetId)}
                      className={`${knownName ? "text-slate-300 hover:text-slate-100" : "mono text-xs text-slate-500 hover:text-slate-300"} transition cursor-pointer`}
                    >
                      {label}
                    </button>
                  </div>
                  {walletBalanceHidden ? (
                    <span className="inline-flex gap-0.5 text-slate-500">
                      <PawIcon />
                      <PawIcon />
                    </span>
                  ) : (
                    <span className="mono text-slate-100">
                      {tp.amount.toLocaleString()}
                    </span>
                  )}
                </div>
              );
            })}
            {tokenTotalPages > 1 && (
              <div className="mt-3 flex items-center justify-between text-xs text-slate-400">
                <button
                  type="button"
                  disabled={walletTokenPage <= 0}
                  onClick={() =>
                    useStore.setState((s) => ({
                      walletTokenPage: Math.max(0, s.walletTokenPage - 1),
                    }))
                  }
                  className={`rounded-lg border border-slate-700 px-2.5 py-1 transition ${walletTokenPage <= 0 ? "text-slate-600 cursor-not-allowed" : "text-slate-300 hover:bg-slate-800"}`}
                >
                  &lsaquo; Prev
                </button>
                <span>
                  {walletTokenPage + 1} / {tokenTotalPages}
                </span>
                <button
                  type="button"
                  disabled={walletTokenPage >= tokenTotalPages - 1}
                  onClick={() =>
                    useStore.setState((s) => ({
                      walletTokenPage: s.walletTokenPage + 1,
                    }))
                  }
                  className={`rounded-lg border border-slate-700 px-2.5 py-1 transition ${walletTokenPage >= tokenTotalPages - 1 ? "text-slate-600 cursor-not-allowed" : "text-slate-300 hover:bg-slate-800"}`}
                >
                  Next &rsaquo;
                </button>
              </div>
            )}
          </div>
        )}

        {/* Limit Orders */}
        {myLimitOrders.length > 0 && (
          <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
            <h3 className="mb-3 font-semibold text-slate-100">Limit Orders</h3>
            {myLimitOrders.map(({ order: o, market: m }) => {
              const truncQ =
                m.question.length > 45
                  ? `${m.question.slice(0, 45)}...`
                  : m.question;
              const cancelling = cancellingOrderId === o.id;
              const dirColor =
                o.direction === "sell-quote"
                  ? "text-emerald-300 bg-emerald-500/20"
                  : "text-red-300 bg-red-500/20";
              const dirText = o.direction === "sell-quote" ? "BUY" : "SELL";
              return (
                <div
                  key={o.id}
                  className="flex items-center justify-between border-b border-slate-800 py-3 text-sm"
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <span
                      className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${dirColor}`}
                    >
                      {dirText}
                    </span>
                    <button
                      type="button"
                      onClick={() => handleOpenMarket(m.id)}
                      className="truncate text-slate-300 hover:text-slate-100 transition cursor-pointer text-left"
                    >
                      {truncQ}
                    </button>
                  </div>
                  <div className="flex items-center gap-3 shrink-0">
                    {walletBalanceHidden ? (
                      <span className="inline-flex items-center h-5 gap-0.5 text-slate-500">
                        <PawIcon />
                        <PawIcon />
                      </span>
                    ) : (
                      <span className="inline-flex items-center h-5 text-xs text-slate-400">
                        {o.price} sats &middot;{" "}
                        {o.offered_amount.toLocaleString()} offered
                      </span>
                    )}
                    <button
                      type="button"
                      onClick={() => handleCancelOrder(o.id)}
                      disabled={cancelling}
                      className={`rounded border px-2 py-0.5 text-xs transition ${cancelling ? "border-slate-700 text-slate-500" : "border-rose-800 text-rose-400 hover:bg-rose-900/30"}`}
                    >
                      {cancelling ? "Cancelling..." : "Cancel"}
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}

        {/* My Pools */}
        {myPools.length > 0 && (
          <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
            <h3 className="mb-3 font-semibold text-slate-100">My Pools</h3>
            {myPools.map((p) => (
              <div
                key={p.pool_id}
                className="flex items-center justify-between border-b border-slate-800 py-3 text-sm"
              >
                <div className="flex items-center gap-2 min-w-0">
                  <span className="mono text-slate-300">
                    {p.pool_id.slice(0, 10)}...
                  </span>
                  <span className="text-xs text-slate-500">
                    {p.market_id.slice(0, 10)}...
                  </span>
                </div>
                <div className="flex items-center gap-3 shrink-0">
                  {walletBalanceHidden ? (
                    <span className="inline-flex items-center h-5 gap-0.5 text-slate-500">
                      <PawIcon />
                      <PawIcon />
                    </span>
                  ) : (
                    <span className="inline-flex items-center h-5 text-xs text-slate-400">
                      Y:{p.reserve_yes} N:{p.reserve_no} L:
                      {p.reserve_collateral}
                    </span>
                  )}
                  <span className="mono text-[10px] text-slate-500">
                    {p.creation_txid.slice(0, 10)}...
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Transactions */}
        <ActivityList walletData={walletData} markets={markets} />

        {/* Swaps */}
        {(walletData?.swaps ?? []).length > 0 && (
          <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
            <h3 className="mb-3 font-semibold text-slate-100">Swaps</h3>
            {(walletData?.swaps ?? []).map((sw) => (
              <div
                key={sw.id}
                className="flex items-center justify-between border-b border-slate-800 py-3 text-sm"
              >
                <div>
                  <span className="text-slate-300">
                    {sw.flow
                      .replace(/_/g, " ")
                      .replace(/\b\w/g, (c) => c.toUpperCase())}
                  </span>
                  {walletBalanceHidden ? (
                    <span className="ml-2 inline-flex items-center h-5 gap-0.5 text-slate-500">
                      <PawIcon />
                      <PawIcon />
                    </span>
                  ) : (
                    <span className="ml-2 inline-flex items-center h-5 text-slate-500">
                      {sw.invoiceAmountSat.toLocaleString()} sats
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-slate-500">
                    {sw.status
                      .replace(/[._]/g, " ")
                      .replace(/\b\w/g, (c) => c.toUpperCase())}
                  </span>
                  <button
                    type="button"
                    onClick={() => {
                      void (async () => {
                        try {
                          const { invoke } = await import(
                            "@tauri-apps/api/core"
                          );
                          await invoke("refresh_payment_swap_status", {
                            swapId: sw.id,
                          });
                          const swaps =
                            await invoke<PaymentSwap[]>("list_payment_swaps");
                          if (walletData) {
                            useStore.setState({
                              walletData: { ...walletData, swaps },
                            });
                          }
                        } catch (e) {
                          useStore.setState({ walletError: String(e) });
                        }
                      })();
                    }}
                    className="rounded border border-slate-700 px-2 py-1 text-xs text-slate-400 hover:bg-slate-800"
                  >
                    Refresh
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {/* UTXOs */}
        <UtxoList utxos={walletData?.utxos ?? []} assetLabel={assetLabel} />
      </div>

      {/* Wallet modal (receive/send) */}
      {walletModal !== "none" && <WalletModal />}
    </div>
  );
}
