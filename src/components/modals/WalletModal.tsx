import { useCallback } from "react";
import { createPortal } from "react-dom";
import { useEscapeKey } from "../../hooks/useEscapeKey";
import { useStore } from "../../store";
import { formatLbtc } from "../../utils-react/wallet";
import { CloseButton } from "../shared/CloseButton";
import { HiddenBalance } from "../shared/PawIcon";
import { ReceiveModal } from "./ReceiveModal";
import { SendModal } from "./SendModal";

export function WalletModal() {
  const walletModal = useStore((s) => s.walletModal);
  const walletModalTab = useStore((s) => s.walletModalTab);
  const walletData = useStore((s) => s.walletData);
  const walletPolicyAssetId = useStore((s) => s.walletPolicyAssetId);
  const walletUnit = useStore((s) => s.walletUnit);
  const walletBalanceHidden = useStore((s) => s.walletBalanceHidden);
  const showLbtcLabel = useStore((s) => s.showLbtcLabel);

  const handleClose = useCallback(() => {
    useStore.setState({
      walletModal: "none",
      receiveAmount: "",
      receiveCreating: false,
      receiveError: "",
      receiveLightningSwap: null,
      receiveBitcoinSwap: null,
      modalQr: "",
      sendInvoice: "",
      sendLiquidAddress: "",
      sendLiquidAmount: "",
      sendBtcAmount: "",
      sendCreating: false,
      sendError: "",
      sentLightningSwap: null,
      sentLiquidResult: null,
      sentBitcoinSwap: null,
    });
  }, []);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.target === e.currentTarget) {
        handleClose();
      }
    },
    [handleClose],
  );

  const handleTabChange = useCallback(
    (tab: "lightning" | "liquid" | "bitcoin") => {
      useStore.setState({ walletModalTab: tab, modalQr: "" });
    },
    [],
  );

  useEscapeKey(walletModal !== "none", handleClose);

  if (walletModal === "none") return null;

  const title = walletModal === "receive" ? "Receive Funds" : "Send Funds";
  const policyBalance =
    walletData && walletPolicyAssetId
      ? (walletData.balance[walletPolicyAssetId] ?? 0)
      : 0;
  const subtitle =
    walletModal === "receive"
      ? "Choose a method to receive funds into your Liquid wallet."
      : "Send funds from your wallet via Lightning, Liquid, or Bitcoin.";

  const tabs: Array<"lightning" | "liquid" | "bitcoin"> = [
    "lightning",
    "liquid",
    "bitcoin",
  ];

  const modal = (
    <div
      role="presentation"
      onClick={handleBackdropClick}
      className="macos-overlay-safe-top fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    >
      <div className="relative mx-4 w-full max-w-md rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
          <div>
            <h3 className="text-lg font-medium text-slate-100">{title}</h3>
            <p className="text-xs text-slate-400">{subtitle}</p>
          </div>
          <CloseButton onClick={handleClose} />
        </div>
        <div className="space-y-4 p-6">
          {walletModal === "send" && (
            <div className="flex items-center justify-between rounded-lg border border-slate-700 bg-slate-900/50 px-4 py-3">
              <span className="text-xs text-slate-400">Available</span>
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium text-slate-200">
                  {walletBalanceHidden ? (
                    <HiddenBalance className="text-sm text-slate-500" />
                  ) : (
                    formatLbtc(policyBalance, walletUnit, showLbtcLabel)
                  )}
                </span>
                <button
                  type="button"
                  onClick={() =>
                    useStore.setState({
                      walletBalanceHidden: !walletBalanceHidden,
                    })
                  }
                  className="text-slate-500 hover:text-slate-300 transition"
                >
                  {walletBalanceHidden ? (
                    <svg
                      aria-hidden="true"
                      className="h-3.5 w-3.5"
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
                      className="h-3.5 w-3.5"
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
            </div>
          )}
          {/* Tab bar */}
          <div className="flex rounded-lg border border-slate-700 bg-slate-900/50 p-1 gap-1">
            {tabs.map((t) => {
              const active = walletModalTab === t;
              const label =
                t === "lightning"
                  ? "Lightning"
                  : t === "liquid"
                    ? "Liquid"
                    : "Bitcoin";
              return (
                <button
                  type="button"
                  key={t}
                  onClick={() => handleTabChange(t)}
                  className={`flex-1 rounded-md px-3 py-2 text-sm font-semibold transition ${active ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}`}
                >
                  {label}
                </button>
              );
            })}
          </div>

          {/* Content */}
          {walletModal === "receive" ? <ReceiveModal /> : <SendModal />}
        </div>
      </div>
    </div>
  );

  return createPortal(modal, document.body);
}
