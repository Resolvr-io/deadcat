import { useCallback } from "react";
import { useStore } from "../../store";
import {
  useCreateWallet,
  useRestoreWallet,
} from "../../queries/mutations/useWalletOps";
import { invoke } from "@tauri-apps/api/core";

function MnemonicGrid({ mnemonic }: { mnemonic: string }) {
  const words = mnemonic.split(" ");
  const rows: string[][] = [];
  for (let i = 0; i < words.length; i += 3) {
    rows.push(words.slice(i, i + 3));
  }

  return (
    <div>
      {rows.map((row, rowIdx) => (
        <div key={rowIdx}>
          <div className="grid grid-cols-3 gap-x-4 py-2.5">
            {row.map((w, colIdx) => (
              <div key={colIdx} className="flex items-baseline gap-1.5 min-w-0">
                <span className="text-xs text-slate-500 shrink-0">
                  {rowIdx * 3 + colIdx + 1}.
                </span>
                <span className="mono text-sm text-slate-100">{w}</span>
              </div>
            ))}
          </div>
          {rowIdx < rows.length - 1 && (
            <div className="border-t border-slate-700/60" />
          )}
        </div>
      ))}
    </div>
  );
}

export function WalletSetup({
  networkBadge,
}: {
  networkBadge: React.ReactNode;
}) {
  const walletMnemonic = useStore((s) => s.walletMnemonic);
  const walletShowRestore = useStore((s) => s.walletShowRestore);
  const walletError = useStore((s) => s.walletError);
  const walletLoading = useStore((s) => s.walletLoading);
  const walletPassword = useStore((s) => s.walletPassword);
  const walletPasswordConfirm = useStore((s) => s.walletPasswordConfirm);
  const walletRestoreMnemonic = useStore((s) => s.walletRestoreMnemonic);
  const nostrNpub = useStore((s) => s.nostrNpub);

  const createWallet = useCreateWallet();
  const restoreWallet = useRestoreWallet();

  const loading = walletLoading || createWallet.isPending || restoreWallet.isPending;

  const handleCreate = useCallback(() => {
    if (!walletPassword || walletPassword !== walletPasswordConfirm) {
      useStore.setState({
        walletError: !walletPassword
          ? "Password is required."
          : "Passwords do not match.",
        walletPassword: "",
        walletPasswordConfirm: "",
      });
      return;
    }
    useStore.setState({ walletLoading: true, walletError: "" });
    createWallet.mutate(
      { password: walletPassword },
      {
        onSuccess: (data) => {
          useStore.setState({
            walletMnemonic: data.mnemonic,
            walletPassword: "",
            walletLoading: false,
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
  }, [walletPassword, walletPasswordConfirm, createWallet]);

  const handleRestore = useCallback(() => {
    if (
      !walletRestoreMnemonic.trim() ||
      !walletPassword ||
      walletPassword !== walletPasswordConfirm
    ) {
      useStore.setState({
        walletError:
          !walletRestoreMnemonic.trim() || !walletPassword
            ? "Recovery phrase and password are required."
            : "Passwords do not match.",
        walletPassword: "",
        walletPasswordConfirm: "",
      });
      return;
    }
    useStore.setState({ walletLoading: true, walletError: "" });
    restoreWallet.mutate(
      { mnemonic: walletRestoreMnemonic, password: walletPassword },
      {
        onSuccess: () => {
          useStore.setState({
            walletRestoreMnemonic: "",
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
  }, [walletRestoreMnemonic, walletPassword, walletPasswordConfirm, restoreWallet]);

  const handleDismissMnemonic = useCallback(() => {
    useStore.setState({ walletMnemonic: "", walletPassword: "" });
  }, []);

  const handleCopyMnemonic = useCallback(() => {
    void navigator.clipboard.writeText(walletMnemonic);
  }, [walletMnemonic]);

  const handleToggleRestore = useCallback(() => {
    useStore.setState({
      walletShowRestore: !walletShowRestore,
      walletError: "",
    });
  }, [walletShowRestore]);

  const handleNostrRestore = useCallback(() => {
    void (async () => {
      try {
        const mnemonic = await invoke<string>("restore_mnemonic_from_nostr", {
          walletName: "My Wallet",
        });
        useStore.setState({
          walletShowRestore: true,
          walletRestoreMnemonic: mnemonic,
        });
      } catch (e) {
        useStore.setState({ walletError: `No backup found: ${String(e)}` });
      }
    })();
  }, []);

  const errorHtml = walletError ? (
    <div className="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">
      {walletError}
    </div>
  ) : null;

  // Mnemonic display screen (after wallet creation)
  if (walletMnemonic) {
    return (
      <div className="px-6 py-6">
        <div className="space-y-6">
          <h2 className="flex items-center gap-2 text-xl font-medium text-slate-100">
            Wallet Created {networkBadge}
          </h2>
          <div className="rounded-lg border border-slate-600 bg-slate-900/40 p-4 space-y-3">
            <p className="text-sm font-medium text-slate-200">
              Back up your recovery phrase in a safe place.
            </p>
            <MnemonicGrid mnemonic={walletMnemonic} />
            <button
              onClick={handleCopyMnemonic}
              className="mt-2 w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800"
            >
              Copy to clipboard
            </button>
          </div>
          {errorHtml}
          <button
            onClick={handleDismissMnemonic}
            className="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"
          >
            I&apos;ve saved my recovery phrase
          </button>
        </div>
      </div>
    );
  }

  const isCreate = !walletShowRestore;
  const isRestore = walletShowRestore;

  return (
    <div className="px-6 py-6">
      <div className="space-y-6">
        <h2 className="flex items-center gap-2 text-xl font-medium text-slate-100">
          Wallet {networkBadge}
        </h2>
        <p className="text-sm text-slate-400">
          Set up a wallet to participate in markets.
        </p>
        {errorHtml}

        {nostrNpub && !loading && (
          <button
            onClick={handleNostrRestore}
            className="w-full rounded-xl border border-slate-700 bg-slate-900/50 p-4 text-left transition hover:border-slate-600"
          >
            <div className="flex items-center gap-3">
              <svg
                className="h-6 w-6 text-slate-500 shrink-0"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth="1.5"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M12 16.5V9.75m0 0l3 3m-3-3l-3 3M6.75 19.5a4.5 4.5 0 01-1.41-8.775 5.25 5.25 0 0110.233-2.33 3 3 0 013.758 3.848A3.752 3.752 0 0118 19.5H6.75z"
                />
              </svg>
              <div>
                <p className="text-sm font-medium text-slate-300">
                  Restore from Nostr Backup
                </p>
                <p className="mt-0.5 text-xs text-slate-500">
                  Fetch encrypted backup from your relays
                </p>
              </div>
            </div>
          </button>
        )}

        <div className="grid grid-cols-2 gap-3">
          <button
            onClick={isCreate || loading ? undefined : handleToggleRestore}
            className={`rounded-xl border ${isCreate ? "border-emerald-500/50 bg-emerald-500/10" : "border-slate-700 bg-slate-900/50 hover:border-slate-600"} p-4 text-left transition ${loading ? "opacity-50 cursor-not-allowed" : ""}`}
            disabled={loading}
          >
            <svg
              className={`h-6 w-6 ${isCreate ? "text-emerald-400" : "text-slate-500"}`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth="1.5"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 4.5v15m7.5-7.5h-15"
              />
            </svg>
            <p
              className={`mt-2 text-sm font-medium ${isCreate ? "text-emerald-300" : "text-slate-300"}`}
            >
              Create New
            </p>
            <p
              className={`mt-0.5 text-xs ${isCreate ? "text-emerald-400/60" : "text-slate-500"}`}
            >
              Generate a fresh wallet
            </p>
          </button>
          <button
            onClick={isRestore || loading ? undefined : handleToggleRestore}
            className={`rounded-xl border ${isRestore ? "border-emerald-500/50 bg-emerald-500/10" : "border-slate-700 bg-slate-900/50 hover:border-slate-600"} p-4 text-left transition ${loading ? "opacity-50 cursor-not-allowed" : ""}`}
            disabled={loading}
          >
            <svg
              className={`h-6 w-6 ${isRestore ? "text-emerald-400" : "text-slate-500"}`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth="1.5"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3"
              />
            </svg>
            <p
              className={`mt-2 text-sm font-medium ${isRestore ? "text-emerald-300" : "text-slate-300"}`}
            >
              Restore
            </p>
            <p
              className={`mt-0.5 text-xs ${isRestore ? "text-emerald-400/60" : "text-slate-500"}`}
            >
              From recovery phrase
            </p>
          </button>
        </div>

        {isCreate ? (
          <div className="space-y-4 rounded-xl border border-slate-700 bg-slate-900/50 p-6">
            <div>
              <label
                htmlFor="wallet-password"
                className="text-xs font-medium text-slate-400"
              >
                Encryption Password
              </label>
              <p className="mt-0.5 text-[11px] text-slate-500">
                Used to encrypt your wallet on this device.
              </p>
            </div>
            <input
              id="wallet-password"
              type="password"
              maxLength={32}
              value={walletPassword}
              onChange={(e) =>
                useStore.setState({ walletPassword: e.target.value })
              }
              placeholder="Enter a password"
              autoComplete="new-password"
              onPaste={(e) => e.preventDefault()}
              className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50"
              disabled={loading}
            />
            <input
              id="wallet-password-confirm"
              type="password"
              maxLength={32}
              value={walletPasswordConfirm}
              onChange={(e) =>
                useStore.setState({ walletPasswordConfirm: e.target.value })
              }
              placeholder="Confirm password"
              autoComplete="new-password"
              onPaste={(e) => e.preventDefault()}
              className={`h-11 w-full rounded-lg border ${walletPasswordConfirm && walletPassword !== walletPasswordConfirm ? "border-red-500/50" : "border-slate-700"} bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50`}
              disabled={loading}
            />
            <button
              onClick={handleCreate}
              className="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={loading}
            >
              {loading ? "Creating..." : "Create Wallet"}
            </button>
          </div>
        ) : (
          <div className="space-y-4 rounded-xl border border-slate-700 bg-slate-900/50 p-6">
            <div>
              <label
                htmlFor="wallet-restore-mnemonic"
                className="text-xs font-medium text-slate-400"
              >
                Recovery Phrase
              </label>
              <p className="mt-0.5 text-[11px] text-slate-500">
                Enter your 12-word recovery phrase to restore your wallet.
              </p>
            </div>
            <textarea
              id="wallet-restore-mnemonic"
              placeholder="word1 word2 word3 ..."
              rows={3}
              value={walletRestoreMnemonic}
              onChange={(e) =>
                useStore.setState({ walletRestoreMnemonic: e.target.value })
              }
              className="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 focus:ring-2 mono disabled:opacity-50"
              disabled={loading}
            />
            <div>
              <label
                htmlFor="wallet-password"
                className="text-xs font-medium text-slate-400"
              >
                Encryption Password
              </label>
              <p className="mt-0.5 text-[11px] text-slate-500">
                Set a password to encrypt the restored wallet.
              </p>
            </div>
            <input
              id="wallet-password"
              type="password"
              maxLength={32}
              value={walletPassword}
              onChange={(e) =>
                useStore.setState({ walletPassword: e.target.value })
              }
              placeholder="Enter a password"
              autoComplete="new-password"
              onPaste={(e) => e.preventDefault()}
              className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50"
              disabled={loading}
            />
            <input
              id="wallet-password-confirm"
              type="password"
              maxLength={32}
              value={walletPasswordConfirm}
              onChange={(e) =>
                useStore.setState({ walletPasswordConfirm: e.target.value })
              }
              placeholder="Confirm password"
              autoComplete="new-password"
              onPaste={(e) => e.preventDefault()}
              className={`h-11 w-full rounded-lg border ${walletPasswordConfirm && walletPassword !== walletPasswordConfirm ? "border-red-500/50" : "border-slate-700"} bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50`}
              disabled={loading}
            />
            <button
              onClick={handleRestore}
              className="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={loading}
            >
              {loading ? "Restoring..." : "Restore Wallet"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
