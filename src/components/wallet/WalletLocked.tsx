import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import { useUnlockWallet } from "../../queries/mutations/useWalletOps";
import { useStore } from "../../store";

export function WalletLocked({
  networkBadge,
}: {
  networkBadge: React.ReactNode;
}) {
  const walletError = useStore((s) => s.walletError);
  const walletLoading = useStore((s) => s.walletLoading);
  const walletPassword = useStore((s) => s.walletPassword);
  const nostrNpub = useStore((s) => s.nostrNpub);

  const [forgotOpen, setForgotOpen] = useState(false);

  const unlockWallet = useUnlockWallet();
  const loading = walletLoading || unlockWallet.isPending;

  const handleUnlock = useCallback(() => {
    if (!walletPassword) {
      useStore.setState({ walletError: "Password is required." });
      return;
    }
    useStore.setState({ walletLoading: true, walletError: "" });
    unlockWallet.mutate(
      { password: walletPassword },
      {
        onSuccess: () => {
          useStore.setState({
            walletPassword: "",
            walletLoading: false,
            walletStatus: "unlocked",
          });
        },
        onError: (e) => {
          useStore.setState({
            walletError: String(e),
            walletLoading: false,
          });
        },
      },
    );
  }, [walletPassword, unlockWallet]);

  const handleForgotDelete = useCallback(() => {
    void (async () => {
      try {
        await invoke("delete_wallet");
        useStore.setState({
          walletData: null,
          walletPassword: "",
          walletMnemonic: "",
          walletError: "",
          walletModal: "none",
          walletStatus: "not_created",
        });
      } catch (e) {
        useStore.setState({
          walletError: `Failed to remove wallet: ${String(e)}`,
        });
      }
    })();
  }, []);

  const errorHtml = walletError ? (
    <div className="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">
      {walletError}
    </div>
  ) : null;

  return (
    <div className="px-6 py-6">
      <div className="space-y-6">
        <h2 className="flex items-center gap-2 text-xl font-medium text-slate-100">
          Wallet {networkBadge}
        </h2>
        <p className="text-sm text-slate-400">
          Wallet locked. Enter your password to unlock.
        </p>
        {errorHtml}
        <div className="space-y-4 rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <input
            id="wallet-password"
            type="password"
            maxLength={32}
            value={walletPassword}
            onChange={(e) =>
              useStore.setState({ walletPassword: e.target.value })
            }
            onKeyDown={(e) => {
              if (e.key === "Enter") handleUnlock();
            }}
            placeholder="Password"
            className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50"
            disabled={loading}
          />
          <button
            type="button"
            onClick={handleUnlock}
            className="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed"
            disabled={loading}
          >
            {loading ? "Unlocking..." : "Unlock"}
          </button>
        </div>
        <details
          className="group"
          open={forgotOpen}
          onToggle={(e) => setForgotOpen((e.target as HTMLDetailsElement).open)}
        >
          <summary className="cursor-pointer text-xs text-slate-500 hover:text-slate-400 transition select-none">
            Forgot your password?
          </summary>
          <div className="mt-3 rounded-lg border border-slate-800 bg-slate-900/50 p-4 space-y-3">
            <p className="text-xs text-slate-400">
              The password protects your wallet on this device only. If
              you&apos;ve forgotten it, you can delete the wallet and restore it
              using either method below.{" "}
              <strong className="text-slate-300">Your funds are safe</strong> as
              long as you have your recovery phrase or nsec.
            </p>
            <div className="space-y-1.5">
              {nostrNpub && (
                <div className="flex items-start gap-2">
                  <svg
                    aria-hidden="true"
                    className="mt-0.5 h-3.5 w-3.5 shrink-0 text-emerald-400"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeWidth="2"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
                    />
                  </svg>
                  <p className="text-xs text-slate-400">
                    <strong className="text-slate-300">
                      Restore from Nostr backup
                    </strong>{" "}
                    -- If you backed up to relays, your nsec is all you need. No
                    password required.
                  </p>
                </div>
              )}
              <div className="flex items-start gap-2">
                <svg
                  aria-hidden="true"
                  className="mt-0.5 h-3.5 w-3.5 shrink-0 text-slate-500"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth="2"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3"
                  />
                </svg>
                <p className="text-xs text-slate-400">
                  <strong className="text-slate-300">
                    Restore from recovery phrase
                  </strong>{" "}
                  -- Enter your 12-word seed phrase and set a new password.
                </p>
              </div>
            </div>
            <button
              type="button"
              onClick={handleForgotDelete}
              className="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition"
            >
              Delete Wallet &amp; Restore
            </button>
          </div>
        </details>
      </div>
    </div>
  );
}
