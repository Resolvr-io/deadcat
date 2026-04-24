import { useCallback } from "react";
import { useEscapeKey } from "../../hooks/useEscapeKey";
import { useLockScroll } from "../../hooks/useLockScroll";
import { useStore } from "../../store";
import { CloseButton } from "../shared/CloseButton";
import { WalletLocked } from "./WalletLocked";
import { WalletSetup } from "./WalletSetup";
import { WalletUnlocked } from "./WalletUnlocked";

export function WalletPage() {
  const walletOpen = useStore((s) => s.walletOpen);
  const walletStatus = useStore((s) => s.walletStatus);
  const walletNetwork = useStore((s) => s.walletNetwork);

  // Clear every wallet-scoped password input + scratch field on
  // close. Inputs read from the store rather than local state, so
  // without this reset a half-typed password survives across
  // close/reopen — confusing at best, and a shoulder-surf risk at
  // worst when the same device is shared.
  const close = useCallback(() => {
    useStore.setState({
      walletOpen: false,
      walletPassword: "",
      walletPasswordConfirm: "",
      walletRestoreMnemonic: "",
      walletMnemonic: "",
      walletShowCreate: false,
      walletShowRestore: false,
      walletError: "",
    });
  }, []);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.target === e.currentTarget) close();
    },
    [close],
  );

  useLockScroll();
  useEscapeKey(walletOpen, close);

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
    content = <WalletLocked />;
  } else {
    content = <WalletUnlocked networkBadge={networkBadge} />;
  }

  return (
    <div
      onClick={handleBackdropClick}
      className="macos-overlay-safe-top fixed inset-0 z-40 flex items-center justify-center bg-black/50 backdrop-blur-sm"
    >
      <div className="relative mx-4 flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        {walletStatus !== "not_created" && (
          <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
            <h2 className="text-lg font-medium text-slate-100">
              Liquid Wallet
            </h2>
            <CloseButton onClick={close} />
          </div>
        )}
        {walletStatus === "not_created" && (
          <CloseButton
            onClick={close}
            className="absolute right-4 top-4 z-10"
          />
        )}
        <div className="flex-1 overflow-y-auto">{content}</div>
      </div>
    </div>
  );
}
