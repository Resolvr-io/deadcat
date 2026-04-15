import { useCallback, useState } from "react";
import {
  useCreateWallet,
  useRestoreWallet,
} from "../../queries/mutations/useWalletOps";
import { useStore } from "../../store";
import { showToast } from "../shared/Toast";

function MnemonicGrid({ mnemonic }: { mnemonic: string }) {
  const words = mnemonic.split(" ");
  const rows: string[][] = [];
  for (let i = 0; i < words.length; i += 3) {
    rows.push(words.slice(i, i + 3));
  }

  return (
    <div>
      {rows.map((row, rowIdx) => (
        <div key={`row-${rowIdx * 3}`}>
          <div className="grid grid-cols-3 gap-x-4 py-2.5">
            {row.map((w, colIdx) => (
              <div
                key={`word-${rowIdx * 3 + colIdx + 1}`}
                className="flex items-baseline gap-1.5 min-w-0"
              >
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
  const [showPw, setShowPw] = useState(false);
  const walletMnemonic = useStore((s) => s.walletMnemonic);
  const walletShowRestore = useStore((s) => s.walletShowRestore);
  const walletShowCreate = useStore((s) => s.walletShowCreate);
  const walletError = useStore((s) => s.walletError);
  const walletLoading = useStore((s) => s.walletLoading);
  const walletPassword = useStore((s) => s.walletPassword);
  const walletPasswordConfirm = useStore((s) => s.walletPasswordConfirm);
  const walletRestoreMnemonic = useStore((s) => s.walletRestoreMnemonic);
  const nostrNpub = useStore((s) => s.nostrNpub);

  const createWallet = useCreateWallet();
  const restoreWallet = useRestoreWallet();

  const loading =
    walletLoading || createWallet.isPending || restoreWallet.isPending;

  const handleCreate = useCallback(() => {
    if (
      !walletPassword ||
      walletPassword.length < 4 ||
      walletPassword !== walletPasswordConfirm
    ) {
      useStore.setState({
        walletError:
          !walletPassword || walletPassword.length < 8
            ? "Password must be at least 8 characters."
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
            walletStatus: "unlocked",
            walletMnemonic: data.mnemonic,
            walletPassword: "",
            walletPasswordConfirm: "",
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
            walletShowRestore: false,
            setupModalOpen: false,
            walletOpen: false,
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
  }, [
    walletRestoreMnemonic,
    walletPassword,
    walletPasswordConfirm,
    restoreWallet,
  ]);

  const handleDismissMnemonic = useCallback(() => {
    useStore.setState({
      walletMnemonic: "",
      walletPassword: "",
      walletShowCreate: false,
      setupModalOpen: false,
      walletOpen: false,
    });
  }, []);

  const handleCopyMnemonic = useCallback(() => {
    void navigator.clipboard.writeText(walletMnemonic);
    showToast("Copied recovery phrase");
  }, [walletMnemonic]);

  const handleToggleRestore = useCallback(() => {
    useStore.setState({
      walletShowRestore: !walletShowRestore,
      walletError: "",
    });
  }, [walletShowRestore]);

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
          <div className="rounded-lg border border-amber-700/30 bg-amber-950/20 px-4 py-3">
            <p className="text-xs text-amber-300/90 leading-relaxed">
              Write down these 12 words and store them safely. This is the only
              way to recover your wallet. Your Nostr key backup does NOT contain
              your wallet.
            </p>
          </div>
          <div className="rounded-lg border border-slate-600 bg-slate-900/40 p-4 space-y-3">
            <MnemonicGrid mnemonic={walletMnemonic} />
            <button
              type="button"
              onClick={handleCopyMnemonic}
              className="mt-2 w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800"
            >
              Copy to clipboard
            </button>
          </div>
          {errorHtml}
          <button
            type="button"
            onClick={handleDismissMnemonic}
            className="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"
          >
            I&apos;ve saved my recovery phrase
          </button>
        </div>
      </div>
    );
  }

  // "none" = show welcome options, "create" = show create form, "restore" = show restore form
  const mode = walletShowRestore
    ? "restore"
    : walletShowCreate
      ? "create"
      : "none";

  // Welcome screen — just the 3 option cards
  if (mode === "none" && !loading) {
    return (
      <div className="px-6 py-6">
        <div className="space-y-6">
          <div>
            <h2 className="text-xl font-medium text-slate-100">
              {nostrNpub ? "Set Up Wallet" : "Welcome to Deadcat"}{" "}
              {networkBadge}
            </h2>
            <p className="mt-1 text-sm text-slate-400">
              {nostrNpub
                ? "Create a new wallet or restore from a recovery phrase."
                : "Set up a self-custody Liquid wallet to start trading prediction markets."}
            </p>
          </div>
          {errorHtml}

          <div className="space-y-3">
            <button
              type="button"
              onClick={() =>
                useStore.setState({
                  walletShowCreate: true,
                  walletShowRestore: false,
                  walletPassword: "",
                  walletPasswordConfirm: "",
                })
              }
              className="w-full rounded-xl border border-slate-700 bg-slate-900/50 p-4 text-left transition hover:border-emerald-500/50 hover:bg-emerald-500/5"
            >
              <div className="flex items-center gap-3">
                <svg
                  aria-hidden="true"
                  className="h-6 w-6 text-emerald-400 shrink-0"
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
                <div>
                  <p className="text-sm font-medium text-slate-200">
                    Create New Wallet
                  </p>
                  <p className="mt-0.5 text-xs text-slate-500">
                    Generate a fresh wallet with a new recovery phrase
                  </p>
                </div>
              </div>
            </button>
            <button
              type="button"
              onClick={handleToggleRestore}
              className="w-full rounded-xl border border-slate-700 bg-slate-900/50 p-4 text-left transition hover:border-slate-500"
            >
              <div className="flex items-center gap-3">
                <svg
                  aria-hidden="true"
                  className="h-6 w-6 text-slate-500 shrink-0"
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
                <div>
                  <p className="text-sm font-medium text-slate-200">
                    Restore from Recovery Phrase
                  </p>
                  <p className="mt-0.5 text-xs text-slate-500">
                    Enter your 12-word phrase to recover an existing wallet
                  </p>
                </div>
              </div>
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Create / Restore form — shown after selecting an option
  const isRestore = walletShowRestore;
  const pwValid = walletPassword.length >= 8;
  const pwMatch = walletPassword === walletPasswordConfirm;
  const canSubmit =
    !loading &&
    pwValid &&
    pwMatch &&
    (!isRestore || walletRestoreMnemonic.trim().length > 0);

  const backToWelcome = () =>
    useStore.setState({
      walletShowCreate: false,
      walletShowRestore: false,
      walletPassword: "",
      walletPasswordConfirm: "",
      walletRestoreMnemonic: "",
      walletError: "",
    });

  return (
    <div className="px-6 py-6">
      <div className="space-y-5">
        <div>
          <button
            type="button"
            onClick={backToWelcome}
            className="mb-3 flex items-center gap-1 text-sm text-slate-400 transition hover:text-slate-200"
          >
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
              <polyline points="15 18 9 12 15 6" />
            </svg>
            Back
          </button>
          <h2 className="text-xl font-medium text-slate-100">
            {isRestore ? "Restore Wallet" : "Create Wallet"} {networkBadge}
          </h2>
        </div>
        {errorHtml}

        {isRestore && (
          <div>
            <label
              htmlFor="wallet-restore-mnemonic"
              className="text-xs font-medium text-slate-400 uppercase tracking-wide"
            >
              Recovery Phrase
            </label>
            <textarea
              id="wallet-restore-mnemonic"
              placeholder="word1 word2 word3 ..."
              rows={3}
              value={walletRestoreMnemonic}
              onChange={(e) =>
                useStore.setState({ walletRestoreMnemonic: e.target.value })
              }
              className="mt-1.5 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 focus:ring-2 mono disabled:opacity-50"
              disabled={loading}
            />
          </div>
        )}

        <div>
          <label
            htmlFor="wallet-password"
            className="text-xs font-medium text-slate-400 uppercase tracking-wide"
          >
            Password
          </label>
          <div className="relative mt-1.5">
            <input
              id="wallet-password"
              type={showPw ? "text" : "password"}
              minLength={8}
              maxLength={32}
              value={walletPassword}
              onChange={(e) =>
                useStore.setState({ walletPassword: e.target.value })
              }
              placeholder="Enter a password"
              autoComplete="new-password"
              className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 pr-11 text-sm outline-none ring-emerald-400 transition focus:ring-2"
              disabled={loading}
            />
            <button
              type="button"
              onClick={() => setShowPw((v) => !v)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 transition"
              tabIndex={-1}
            >
              {showPw ? (
                <svg
                  aria-hidden="true"
                  className="h-5 w-5"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth="1.5"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M3.98 8.223A10.477 10.477 0 001.934 12C3.226 16.338 7.244 19.5 12 19.5c.993 0 1.953-.138 2.863-.395M6.228 6.228A10.45 10.45 0 0112 4.5c4.756 0 8.773 3.162 10.065 7.498a10.523 10.523 0 01-4.293 5.774M6.228 6.228L3 3m3.228 3.228l3.65 3.65m7.894 7.894L21 21m-3.228-3.228l-3.65-3.65m0 0a3 3 0 10-4.243-4.243m4.242 4.242L9.88 9.88"
                  />
                </svg>
              ) : (
                <svg
                  aria-hidden="true"
                  className="h-5 w-5"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth="1.5"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M2.036 12.322a1.012 1.012 0 010-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178z"
                  />
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                  />
                </svg>
              )}
            </button>
          </div>
          <div className="relative mt-2">
            <input
              id="wallet-password-confirm"
              type={showPw ? "text" : "password"}
              maxLength={32}
              value={walletPasswordConfirm}
              onChange={(e) =>
                useStore.setState({ walletPasswordConfirm: e.target.value })
              }
              placeholder="Confirm password"
              autoComplete="new-password"
              className={`h-11 w-full rounded-lg border ${walletPasswordConfirm && !pwMatch ? "border-red-500/50" : "border-slate-700"} bg-slate-900 px-4 pr-11 text-sm outline-none ring-emerald-400 transition focus:ring-2`}
              disabled={loading}
            />
          </div>
          <div className="mt-1.5 h-5">
            {walletPassword && !pwValid ? (
              <p className="text-xs text-amber-300">Minimum 4 characters</p>
            ) : walletPasswordConfirm && !pwMatch ? (
              <p className="text-xs text-rose-400">
                Passwords don&apos;t match
              </p>
            ) : null}
          </div>
          <div className="mt-2 flex gap-2 rounded-lg border border-amber-700/30 bg-amber-950/20 px-3 py-2.5">
            <svg
              aria-hidden="true"
              className="h-4 w-4 shrink-0 text-amber-400 mt-0.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
              />
            </svg>
            <p className="text-xs text-amber-300/90 leading-relaxed">
              You&apos;ll need this password to unlock your wallet and to
              restore from your backup file.
            </p>
          </div>
        </div>

        <button
          type="button"
          onClick={isRestore ? handleRestore : handleCreate}
          disabled={!canSubmit}
          className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {loading ? (
            <span className="inline-flex items-center">
              {isRestore ? "Restoring" : "Creating"}
              <span className="ml-0.5 inline-flex">
                <span className="loading-dot">.</span>
                <span className="loading-dot">.</span>
                <span className="loading-dot">.</span>
              </span>
            </span>
          ) : isRestore ? (
            "Restore Wallet"
          ) : (
            "Create Wallet"
          )}
        </button>
      </div>
    </div>
  );
}
