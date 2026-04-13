import { invoke } from "@tauri-apps/api/core";
import { useCallback, useMemo, useState } from "react";
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
import { showToast } from "../shared/Toast";
import { ActivityList } from "./ActivityList";
import { UtxoList } from "./UtxoList";

const PAW_ICON = (
  <svg
    aria-hidden="true"
    className="inline-block h-[1em] w-[1em] align-text-bottom"
    viewBox="0 0 90 79"
    fill="currentColor"
  >
    <path d="M26.62,28.27c4.09,2.84,9.4,2.58,12.27-.69,2.3-2.63,3.06-5.82,3.08-10-.35-5.03-1.89-10.34-6.28-14.44C29.51-2.63,21.1-.1,19.06,8.08c-1.74,6.91,1.71,16.11,7.56,20.18Z" />
    <path d="M22.98,41.99c.21-1.73.04-3.62-.43-5.3-1.46-5.21-4-9.77-9.08-12.33C7.34,21.27-.31,24.39,0,32.36c-.03,7.11,5.17,14.41,11.8,16.58,5.57,1.82,10.49-1.16,11.17-6.95Z" />
    <path d="M63.4,28.27c5.85-4.06,9.3-13.26,7.57-20.19C68.92-.12,60.51-2.64,54.33,3.13c-4.4,4.1-5.93,9.41-6.28,14.44.02,4.18.78,7.37,3.08,10,2.87,3.28,8.17,3.54,12.27.7Z" />
    <path d="M76.54,24.36c-5.08,2.56-7.62,7.12-9.08,12.33-.47,1.68-.63,3.57-.43,5.3.69,5.79,5.61,8.77,11.16,6.96,6.63-2.17,11.83-9.47,11.8-16.58.32-7.99-7.32-11.1-13.45-8.01Z" />
    <path d="M65.95,49.84c-2.36-2.86-4.3-6.01-6.45-9.02-.89-1.24-1.8-2.47-2.78-3.65-2.76-3.35-7.24-5.02-11.72-5.02s-8.96,1.68-11.72,5.02c-.98,1.19-1.89,2.41-2.78,3.65-2.15,3.01-4.08,6.15-6.45,9.02-1.77,2.15-4.25,3.82-6.11,5.92-4.14,4.69-4.72,9.96-1.94,15.3,2.79,5.37,8.01,7.6,14.41,7.9,4.82.23,9.23-1.95,13.98-2.16.22-.01.42-.01.62-.01s.4,0,.61.01c4.75.21,9.16,2.38,13.98,2.16,6.39-.3,11.62-2.53,14.41-7.9,2.77-5.34,2.2-10.61-1.94-15.3-1.87-2.1-4.35-3.77-6.12-5.92Z" />
  </svg>
);

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
  const nostrBackupStatus = useStore((s) => s.nostrBackupStatus);
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const myPools = useStore((s) => s.myPools);
  const cancellingOrderId = useStore((s) => s.cancellingOrderId);
  const walletModal = useStore((s) => s.walletModal);

  const { data: markets = [] } = useMarkets();

  const syncWallet = useSyncWallet();
  const lockWallet = useLockWallet();

  const loading = walletLoading || syncWallet.isPending;

  const policyBalance =
    walletData && walletPolicyAssetId
      ? (walletData.balance[walletPolicyAssetId] ?? 0)
      : 0;

  const showBackupBadge =
    !walletData?.backedUp && nostrBackupStatus?.has_backup === false;

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

  const [backupOpen, setBackupOpen] = useState(false);
  const [backupPassword, setBackupPassword] = useState("");
  const [backupWords, setBackupWords] = useState<string[]>([]);
  const [backupLoading, setBackupLoading] = useState(false);
  const [backupError, setBackupError] = useState("");
  const [backupCopied, setBackupCopied] = useState(false);

  const handleShowBackup = useCallback(() => {
    setBackupOpen(true);
    setBackupPassword("");
    setBackupWords([]);
    setBackupError("");
    setBackupCopied(false);
  }, []);

  const handleHideBackup = useCallback(() => {
    setBackupOpen(false);
    setBackupPassword("");
    setBackupWords([]);
    setBackupError("");
    setBackupCopied(false);
  }, []);

  const handleExportBackup = useCallback(async () => {
    if (!backupPassword) {
      setBackupError("Password is required to export recovery phrase.");
      return;
    }
    setBackupLoading(true);
    setBackupError("");
    try {
      const count = await invoke<number>("get_mnemonic_word_count", {
        password: backupPassword,
      });
      const words: string[] = [];
      for (let i = 0; i < count; i++) {
        words.push(
          await invoke<string>("get_mnemonic_word", {
            password: backupPassword,
            index: i,
          }),
        );
      }
      setBackupWords(words);
    } catch (e) {
      setBackupError(String(e));
    }
    setBackupLoading(false);
  }, [backupPassword]);

  const handleCopyBackupMnemonic = useCallback(() => {
    if (backupWords.length > 0) {
      void navigator.clipboard.writeText(backupWords.join(" "));
      setBackupCopied(true);
      showToast("Recovery phrase copied", "success");
    }
  }, [backupWords]);

  const handleOpenExplorerAsset = useCallback(
    (assetId: string) => {
      const base =
        walletNetwork === "testnet"
          ? "https://blockstream.info/liquidtestnet"
          : "https://blockstream.info/liquid";
      window.open(`${base}/asset/${assetId}`, "_blank");
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
        {/* Header */}
        <div className="flex items-center justify-between">
          <h2 className="flex items-center gap-2 text-xl font-medium text-slate-100">
            {networkBadge}
          </h2>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={handleSync}
              className="relative rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800"
              disabled={loading}
            >
              Sync
              {loading && (
                <span className="absolute inset-0 flex items-center justify-center rounded-lg bg-slate-800">
                  <span className="h-3.5 w-3.5 rounded-full border-2 border-transparent border-t-emerald-400 animate-[spin_0.8s_steps(8)_infinite]" />
                </span>
              )}
            </button>
            <button
              type="button"
              onClick={handleShowBackup}
              className={`rounded-lg border px-4 py-2 text-sm transition ${showBackupBadge ? "border-amber-500/40 text-amber-200 hover:bg-amber-500/10" : "border-slate-700 text-slate-300 hover:bg-slate-800"}`}
            >
              <span className="flex items-center gap-2">
                <span>Backup</span>
                {showBackupBadge && (
                  <span className="h-2 w-2 rounded-full bg-amber-300" />
                )}
              </span>
            </button>
            <button
              type="button"
              onClick={handleLock}
              className="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800"
            >
              Lock
            </button>
          </div>
        </div>

        {errorHtml}

        {/* Balance */}
        <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-6 text-center">
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
              <span className="inline-flex items-center gap-1.5 text-3xl text-slate-500">
                {PAW_ICON}
                {PAW_ICON}
                {PAW_ICON}
                {PAW_ICON}
              </span>
            ) : (
              <span className="text-3xl">
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
                        {PAW_ICON}
                        {PAW_ICON}
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
                      {PAW_ICON}
                      {PAW_ICON}
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
                        {PAW_ICON}
                        {PAW_ICON}
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
                      {PAW_ICON}
                      {PAW_ICON}
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
        <ActivityList
          walletData={walletData}
          markets={markets}
          pawIcon={PAW_ICON}
        />

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
                      {PAW_ICON}
                      {PAW_ICON}
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

      {/* Backup modal */}
      {backupOpen && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-2xl border border-slate-800 bg-slate-950 p-6">
            <div className="mb-4 flex items-center justify-between">
              <h3 className="text-lg font-medium text-slate-100">
                Backup Recovery Phrase
              </h3>
              <button
                type="button"
                onClick={handleHideBackup}
                className="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
              >
                <svg
                  aria-hidden="true"
                  className="h-5 w-5"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={2}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </button>
            </div>

            {backupError && (
              <p className="mb-3 rounded-lg bg-rose-500/10 px-3 py-2 text-xs text-rose-300">
                {backupError}
              </p>
            )}

            {backupWords.length > 0 ? (
              <>
                <div className="mb-4 grid grid-cols-3 gap-2">
                  {backupWords.map((word, i) => (
                    <div
                      key={`backup-word-${i + 1}`}
                      className="rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-center text-sm"
                    >
                      <span className="mr-1 text-slate-500">{i + 1}.</span>
                      <span className="font-mono text-slate-200">{word}</span>
                    </div>
                  ))}
                </div>
                <div className="flex gap-3">
                  <button
                    type="button"
                    onClick={handleCopyBackupMnemonic}
                    className={`flex-1 rounded-xl border py-2.5 text-sm font-medium transition ${
                      backupCopied
                        ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-200"
                        : "border-slate-700 text-slate-300 hover:border-slate-500 hover:text-slate-100"
                    }`}
                  >
                    {backupCopied ? "Copied" : "Copy to clipboard"}
                  </button>
                  <button
                    type="button"
                    onClick={handleHideBackup}
                    className="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100"
                  >
                    Done
                  </button>
                </div>
              </>
            ) : (
              <>
                <p className="mb-3 text-sm text-slate-400">
                  Enter your wallet password to reveal your recovery phrase.
                </p>
                <input
                  type="password"
                  maxLength={32}
                  value={backupPassword}
                  onChange={(e) => setBackupPassword(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleExportBackup();
                  }}
                  placeholder="Wallet password"
                  className="mb-3 h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm text-slate-200 outline-none ring-emerald-400 transition focus:ring-2"
                />
                <button
                  type="button"
                  onClick={() => void handleExportBackup()}
                  disabled={backupLoading || !backupPassword}
                  className="w-full rounded-xl bg-emerald-500 py-2.5 text-sm font-semibold text-white transition hover:bg-emerald-400 disabled:opacity-50"
                >
                  {backupLoading ? "Decrypting..." : "Reveal Recovery Phrase"}
                </button>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
