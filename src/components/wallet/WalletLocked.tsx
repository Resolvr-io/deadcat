import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import { useUnlockWallet } from "../../queries/mutations/useWalletOps";
import { useStore } from "../../store";

export function WalletLocked() {
  const walletError = useStore((s) => s.walletError);
  const walletLoading = useStore((s) => s.walletLoading);
  const walletPassword = useStore((s) => s.walletPassword);
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
              The password encrypts your wallet on this device only. If
              you&apos;ve forgotten it, you can delete the wallet and re-create
              it from your{" "}
              <strong className="text-slate-300">
                12-word recovery phrase
              </strong>
              . Your funds are safe as long as you have that phrase.
            </p>
            <p className="text-xs text-slate-400">
              After deleting, the onboarding flow will let you restore your
              wallet and identity from a backup file or by entering your keys
              manually.
            </p>
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
