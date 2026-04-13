import { useCallback } from "react";
import { useStore } from "../../store";
import type { WalletUtxo } from "../../types";
import { btcLabel, formatLbtc } from "../../utils-react/wallet";

type WalletAssetLabel = {
  side: string;
  question: string;
  marketId: string;
};

export function UtxoList({
  utxos,
  assetLabel,
}: {
  utxos: WalletUtxo[];
  assetLabel: Map<string, WalletAssetLabel>;
}) {
  const walletUtxosExpanded = useStore((s) => s.walletUtxosExpanded);
  const walletBalanceHidden = useStore((s) => s.walletBalanceHidden);
  const walletPolicyAssetId = useStore((s) => s.walletPolicyAssetId);
  const walletUnit = useStore((s) => s.walletUnit);
  const walletNetwork = useStore((s) => s.walletNetwork);
  const showLbtcLabel = useStore((s) => s.showLbtcLabel);

  const handleToggle = useCallback(() => {
    useStore.setState((s) => ({
      walletUtxosExpanded: !s.walletUtxosExpanded,
    }));
  }, []);

  const handleOpenMarket = useCallback((marketId: string) => {
    useStore.setState({
      selectedMarketId: marketId,
      view: "detail",
      walletOpen: false,
    });
  }, []);

  if (walletBalanceHidden || utxos.length === 0) return null;

  const explorerBase =
    walletNetwork === "testnet"
      ? "https://blockstream.info/liquidtestnet"
      : "https://blockstream.info/liquid";

  const lbtcUtxos = utxos.filter((u) => u.assetId === walletPolicyAssetId);
  const tokenUtxos = utxos.filter((u) => u.assetId !== walletPolicyAssetId);

  const renderUtxoRow = (u: WalletUtxo, labelContent: React.ReactNode) => {
    const shortOutpoint = `${u.txid.slice(0, 8)}...${u.txid.slice(-4)}:${u.vout}`;
    const conf = u.height !== null ? String(u.height) : "unconfirmed";
    const valueStr =
      u.assetId === walletPolicyAssetId
        ? formatLbtc(u.value, walletUnit, showLbtcLabel)
        : u.value.toLocaleString();

    return (
      <div
        key={`${u.txid}:${u.vout}`}
        className="flex items-center justify-between border-b border-slate-800 py-2 text-xs"
      >
        <div className="flex items-center gap-2 min-w-0">
          {labelContent}
          <a
            href={`${explorerBase}/tx/${u.txid}`}
            target="_blank"
            rel="noopener noreferrer"
            className="mono text-slate-500 hover:text-slate-300 transition truncate"
          >
            {shortOutpoint}
          </a>
          <span className="text-slate-600">{conf}</span>
        </div>
        <span className="mono text-slate-300 shrink-0 ml-2">{valueStr}</span>
      </div>
    );
  };

  return (
    <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
      <button
        type="button"
        onClick={handleToggle}
        className="flex w-full items-center justify-between"
      >
        <h3 className="font-semibold text-slate-100">
          UTXOs{" "}
          <span className="ml-1 text-xs font-normal text-slate-500">
            ({utxos.length})
          </span>
        </h3>
        <svg
          aria-hidden="true"
          className={`h-4 w-4 text-slate-400 transition ${walletUtxosExpanded ? "rotate-180" : ""}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth="2"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>
      {walletUtxosExpanded && (
        <div className="mt-3">
          {lbtcUtxos.length > 0 && (
            <>
              <div className="mb-1 text-[10px] font-medium uppercase tracking-wider text-slate-500">
                {btcLabel(showLbtcLabel)}
              </div>
              {lbtcUtxos.map((u) =>
                renderUtxoRow(
                  u,
                  <span className="rounded bg-slate-700 px-1.5 py-0.5 text-[10px] font-medium text-slate-300 shrink-0">
                    {btcLabel(showLbtcLabel)}
                  </span>,
                ),
              )}
            </>
          )}
          {tokenUtxos.length > 0 && (
            <>
              <div className="mt-3 mb-1 text-[10px] font-medium uppercase tracking-wider text-slate-500">
                Tokens
              </div>
              {tokenUtxos.map((u) => {
                const info = assetLabel.get(u.assetId);
                if (info) {
                  const sideColor =
                    info.side === "YES" ? "text-emerald-300" : "text-red-300";
                  const sideBg =
                    info.side === "YES" ? "bg-emerald-500/20" : "bg-red-500/20";
                  const truncQ =
                    info.question.length > 35
                      ? `${info.question.slice(0, 35)}...`
                      : info.question;
                  return renderUtxoRow(
                    u,
                    <>
                      <span
                        className={`rounded ${sideBg} px-1.5 py-0.5 text-[10px] font-medium ${sideColor} shrink-0`}
                      >
                        {info.side}
                      </span>
                      <button
                        type="button"
                        onClick={() => handleOpenMarket(info.marketId)}
                        className="text-slate-400 hover:text-slate-200 transition cursor-pointer truncate text-left"
                      >
                        {truncQ}
                      </button>
                    </>,
                  );
                }
                const shortAsset = `${u.assetId.slice(0, 8)}...${u.assetId.slice(-4)}`;
                return renderUtxoRow(
                  u,
                  <span className="mono text-slate-500 shrink-0">
                    {shortAsset}
                  </span>,
                );
              })}
            </>
          )}
        </div>
      )}
    </div>
  );
}
