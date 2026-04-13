import { useCallback } from "react";
import { createPortal } from "react-dom";
import { useStore } from "../../store";
import { ReceiveModal } from "./ReceiveModal";
import { SendModal } from "./SendModal";

export function WalletModal() {
  const walletModal = useStore((s) => s.walletModal);
  const walletModalTab = useStore((s) => s.walletModalTab);

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

  if (walletModal === "none") return null;

  const title = walletModal === "receive" ? "Receive Funds" : "Send Funds";
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
      onKeyDown={(e) => {
        if (e.key === "Escape") handleClose();
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    >
      <div className="relative mx-4 w-full max-w-md rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
          <div>
            <h3 className="text-lg font-medium text-slate-100">{title}</h3>
            <p className="text-xs text-slate-400">{subtitle}</p>
          </div>
          <button
            type="button"
            onClick={handleClose}
            className="rounded-lg p-2 text-slate-400 hover:bg-slate-800 hover:text-slate-200"
          >
            <svg
              aria-hidden="true"
              xmlns="http://www.w3.org/2000/svg"
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
        <div className="space-y-4 p-6">
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
