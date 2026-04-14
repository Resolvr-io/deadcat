import { useCallback } from "react";
import { useStore } from "../../store";
import { WalletLocked } from "./WalletLocked";
import { WalletSetup } from "./WalletSetup";
import { WalletUnlocked } from "./WalletUnlocked";

export function WalletPage() {
  const walletOpen = useStore((s) => s.walletOpen);
  const walletStatus = useStore((s) => s.walletStatus);
  const walletNetwork = useStore((s) => s.walletNetwork);

  const close = useCallback(() => {
    useStore.setState({ walletOpen: false });
  }, []);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.target === e.currentTarget) {
        useStore.setState({ walletOpen: false });
      }
    },
    [],
  );

  if (!walletOpen) return null;

  const networkBadge =
    walletNetwork !== "mainnet" ? (
      <span className="rounded-full bg-amber-500/20 px-2.5 py-0.5 text-xs font-medium text-amber-300">
        {walletNetwork}
      </span>
    ) : null;

  let content: React.ReactNode;
  if (walletStatus === "not_created") {
    content = <WalletSetup networkBadge={networkBadge} />;
  } else if (walletStatus === "locked") {
    content = <WalletLocked networkBadge={networkBadge} />;
  } else {
    content = <WalletUnlocked networkBadge={networkBadge} />;
  }

  return (
    <div
      onClick={handleBackdropClick}
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/50 backdrop-blur-sm"
    >
      <div className="relative mx-4 flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        {walletStatus !== "not_created" && (
          <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
            <h2 className="text-lg font-medium text-slate-100">Wallet</h2>
            <button
              type="button"
              onClick={close}
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
        )}
        {walletStatus === "not_created" && (
          <button
            type="button"
            onClick={close}
            className="absolute right-4 top-4 z-10 rounded-lg p-2 text-slate-400 hover:bg-slate-800 hover:text-slate-200"
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
        )}
        <div className="flex-1 overflow-y-auto">{content}</div>
      </div>
    </div>
  );
}
