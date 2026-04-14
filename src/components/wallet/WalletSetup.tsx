import { useCallback } from "react";
import {
  useCreateWallet,
  useRestoreWallet,
} from "../../queries/mutations/useWalletOps";
import { useStore } from "../../store";

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
  const walletMnemonic = useStore((s) => s.walletMnemonic);
  const walletShowRestore = useStore((s) => s.walletShowRestore);
  const walletShowCreate = useStore((s) => s.walletShowCreate);
  const walletError = useStore((s) => s.walletError);
  const walletLoading = useStore((s) => s.walletLoading);
  const walletPassword = useStore((s) => s.walletPassword);
  const walletPasswordConfirm = useStore((s) => s.walletPasswordConfirm);
  const walletRestoreMnemonic = useStore((s) => s.walletRestoreMnemonic);

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
          !walletPassword || walletPassword.length < 4
            ? "Password must be at least 4 characters."
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
          <div className="flex items-start gap-4">
            <svg
              aria-hidden="true"
              className="h-10 w-10 shrink-0 text-emerald-400"
              viewBox="0 0 260 267"
              fill="currentColor"
            >
              <path d="M0.146484 9.04605C0.146484 1.23441 10.9146 -3.16002 16.7881 2.6984L86.5566 71.7336C100.142 68.0294 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0306 173.453 71.7365L243.212 2.71207C249.085 -3.14676 259.854 1.24698 259.854 9.05875V161.26C259.949 162.835 260 164.42 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C1.54644e-06 164.42 0.0506677 162.835 0.146484 161.26V9.04605ZM100.287 187.013L120.892 207.087V208.903C120.892 217.907 114.199 225.23 105.974 225.231H91.0049C87.1409 225.231 84.0001 228.319 84 232.118C84 235.918 87.1446 239.013 91.0049 239.013H105.974C114.534 239.013 122.574 235.049 128.02 228.383C133.461 235.045 141.502 239.013 150.065 239.013C166.019 239.013 179 225.506 179 208.903C179 205.104 175.856 202.013 171.992 202.013C168.128 202.013 164.984 205.104 164.983 208.903C164.983 217.907 158.291 225.231 150.065 225.231C141.84 225.231 135.147 217.907 135.147 208.903V207.049L155.713 187.013H100.287ZM70.4697 140.12L52.4219 122.072L44 130.495L62.0469 148.542L44.0596 166.53L52.4824 174.953L70.4697 156.965L88.5176 175.013L96.9404 166.591L78.8916 148.542L97 130.435L88.5781 122.013L70.4697 140.12ZM195.367 123.557C200.554 128.783 204 138.006 204 148.513C204 158.3 201.01 166.973 196.408 172.339C216.243 169.73 231 159.83 231 148.013C231 135.99 215.724 125.951 195.367 123.557ZM175.489 123.7C155.707 126.33 141 136.217 141 148.013C141 159.603 155.197 169.349 174.456 172.181C169.931 166.803 167 158.204 167 148.513C167 138.102 170.382 128.951 175.489 123.7Z" />
            </svg>
            <div>
              <h2 className="text-xl font-medium text-slate-100">
                Welcome to Deadcat {networkBadge}
              </h2>
              <p className="mt-1 text-sm text-slate-400">
                Set up a self-custody Liquid wallet to start trading prediction
                markets.
              </p>
            </div>
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
      <div className="space-y-6">
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
              className="text-xs font-medium text-slate-400"
            >
              Recovery Phrase
            </label>
            <p className="mt-0.5 text-[11px] text-slate-500">
              Enter your 12-word recovery phrase.
            </p>
            <textarea
              id="wallet-restore-mnemonic"
              placeholder="word1 word2 word3 ..."
              rows={3}
              value={walletRestoreMnemonic}
              onChange={(e) =>
                useStore.setState({ walletRestoreMnemonic: e.target.value })
              }
              className="mt-2 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 focus:ring-2 mono disabled:opacity-50"
              disabled={loading}
            />
          </div>
        )}

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
            className="mt-2 h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50"
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
            className={`mt-2 h-11 w-full rounded-lg border ${walletPasswordConfirm && walletPassword !== walletPasswordConfirm ? "border-red-500/50" : "border-slate-700"} bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50`}
            disabled={loading}
          />
        </div>

        <button
          type="button"
          onClick={isRestore ? handleRestore : handleCreate}
          className="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed"
          disabled={loading}
        >
          {loading
            ? isRestore
              ? "Restoring..."
              : "Creating..."
            : isRestore
              ? "Restore Wallet"
              : "Create Wallet"}
        </button>
      </div>
    </div>
  );
}
