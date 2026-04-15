import { useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { type ReactNode, useCallback } from "react";
import { useStore } from "../../store";
import { showToast } from "../shared/Toast";

function BackButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="mb-8 flex items-center gap-1.5 text-xs text-slate-500 hover:text-slate-300 transition"
    >
      <svg
        className="h-3.5 w-3.5"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={2}
        aria-hidden="true"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M15 19l-7-7 7-7"
        />
      </svg>
      Back
    </button>
  );
}

// ── Mnemonic grid (React version) ───────────────────────────────────
function MnemonicGrid({ mnemonic }: { mnemonic: string }) {
  const words = mnemonic.split(" ");
  const rows: string[][] = [];
  for (let i = 0; i < words.length; i += 3) {
    rows.push(words.slice(i, i + 3));
  }
  return (
    <div className="space-y-2">
      {rows.map((row, rowIdx) => (
        <div key={`row-${rowIdx}`} className="grid grid-cols-3 gap-2">
          {row.map((word, colIdx) => (
            <div
              key={`word-${rowIdx * 3 + colIdx + 1}-${word}`}
              className="flex items-baseline gap-1.5 min-w-0"
            >
              <span className="text-xs text-slate-500 shrink-0">
                {rowIdx * 3 + colIdx + 1}.
              </span>
              <span className="mono text-sm text-slate-100">{word}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

// ── Password fields ─────────────────────────────────────────────────
export function PasswordFields({
  password,
  confirm,
  revealed,
  disabled,
  onPasswordChange,
  onConfirmChange,
  onToggleReveal,
}: {
  password: string;
  confirm: string;
  revealed: boolean;
  disabled: boolean;
  onPasswordChange: (val: string) => void;
  onConfirmChange: (val: string) => void;
  onToggleReveal: () => void;
}) {
  const inputType = revealed ? "text" : "password";

  return (
    <div className="space-y-4">
      <div className="space-y-1.5">
        <label
          htmlFor="wallet-password"
          className="text-xs font-medium text-slate-400 uppercase tracking-wide"
        >
          Password
        </label>
        <div className="relative">
          <input
            id="wallet-password"
            type={inputType}
            maxLength={32}
            placeholder="At least 8 characters"
            autoComplete="new-password"
            onPaste={(e) => e.preventDefault()}
            value={password}
            onChange={(e) => onPasswordChange(e.target.value)}
            disabled={disabled}
            className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 pr-11 pl-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50"
          />
          <button
            type="button"
            onClick={onToggleReveal}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 transition"
            tabIndex={-1}
            disabled={disabled}
          >
            {revealed ? (
              <svg
                className="h-4 w-4"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
                aria-hidden="true"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21"
                />
              </svg>
            ) : (
              <svg
                className="h-4 w-4"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
                aria-hidden="true"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                />
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
                />
              </svg>
            )}
          </button>
        </div>
      </div>
      <div className="space-y-1.5">
        <label
          htmlFor="wallet-password-confirm"
          className="text-xs font-medium text-slate-400 uppercase tracking-wide"
        >
          Confirm password
        </label>
        <input
          id="wallet-password-confirm"
          type={inputType}
          maxLength={32}
          placeholder="Repeat your password"
          autoComplete="new-password"
          onPaste={(e) => e.preventDefault()}
          value={confirm}
          onChange={(e) => onConfirmChange(e.target.value)}
          disabled={disabled}
          className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50"
        />
      </div>
    </div>
  );
}

// ── Finish onboarding helper ────────────────────────────────────────
async function finishOnboarding(
  queryClient: ReturnType<typeof useQueryClient>,
) {
  useStore.setState({
    setupModalOpen: false,
    setupRequires: null,
    onboardingStep: null,
    onboardingWalletOnly: false,
    onboardingWalletName: "My Wallet",
    onboardingSelectedWalletDTag: "",
    onboardingPendingPubkey: "",
    onboardingPendingNpub: "",
    onboardingWalletPassword: "",
    onboardingWalletMnemonic: "",
    onboardingNostrNsec: "",
    onboardingNostrGeneratedNsec: "",
    onboardingNsecRevealed: false,
    onboardingNostrDone: false,
    onboardingError: "",
    onboardingBackupFound: false,
    onboardingBackupScanning: false,
    walletOpen: true,
  });

  // Invalidate queries so React Query re-fetches fresh data
  void queryClient.invalidateQueries({ queryKey: ["walletStatus"] });
  void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
  void queryClient.invalidateQueries({ queryKey: ["markets"] });
  void queryClient.invalidateQueries({ queryKey: ["identity"] });
  void queryClient.invalidateQueries({ queryKey: ["nostrProfile"] });
  void queryClient.invalidateQueries({ queryKey: ["relays"] });
  void queryClient.invalidateQueries({ queryKey: ["nostrBackup"] });
}

// ── Main component ──────────────────────────────────────────────────
interface WalletSetupStepProps {
  stepIndicator: ReactNode;
}

export default function WalletSetupStep({
  stepIndicator,
}: WalletSetupStepProps) {
  const walletMode = useStore((s) => s.onboardingWalletMode);
  const walletOnly = useStore((s) => s.onboardingWalletOnly);
  const loading = useStore((s) => s.onboardingLoading);
  const error = useStore((s) => s.onboardingError);
  const mnemonic = useStore((s) => s.onboardingWalletMnemonic);
  const passwordStep = useStore((s) => s.onboardingWalletPasswordStep);
  const password = useStore((s) => s.onboardingWalletPassword);
  const passwordConfirm = useStore((s) => s.onboardingWalletPasswordConfirm);
  const passwordRevealed = useStore((s) => s.onboardingPasswordRevealed);
  const verifyStep = useStore((s) => s.onboardingMnemonicVerifyStep);
  const verifyIndices = useStore((s) => s.onboardingMnemonicVerifyIndices);
  const verifyInputs = useStore((s) => s.onboardingMnemonicVerifyInputs);
  const backupScanning = useStore((s) => s.onboardingBackupScanning);
  const backupFound = useStore((s) => s.onboardingBackupFound);
  const backupStatus = useStore((s) => s.nostrBackupStatus);
  const selectedDTag = useStore((s) => s.onboardingSelectedWalletDTag);
  const relays = useStore((s) => s.relays);
  const queryClient = useQueryClient();

  const errorHtml = error ? (
    <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3">
      <p className="text-sm text-red-400">{error}</p>
    </div>
  ) : null;

  // ── Back handler ──────────────────────────────────────────────────
  const handleBack = useCallback(() => {
    useStore.setState({ onboardingPasswordRevealed: false });

    if (passwordStep && walletMode === "create") {
      // Back from password page (create) -> verify step
      useStore.setState({
        onboardingWalletPasswordStep: false,
        onboardingWalletPassword: "",
        onboardingWalletPasswordConfirm: "",
        onboardingMnemonicVerifyStep: true,
        onboardingError: "",
      });
    } else if (
      passwordStep &&
      (walletMode === "restore" || walletMode === "nostr-restore")
    ) {
      // Back from password page (restore/nostr-restore) -> sub-page
      useStore.setState({
        onboardingWalletPasswordStep: false,
        onboardingWalletPassword: "",
        onboardingWalletPasswordConfirm: "",
        onboardingError: "",
      });
    } else if (walletMode === "nostr-restore") {
      if (walletOnly) {
        useStore.setState({
          onboardingWalletMode: "create",
          onboardingBackupFound: false,
          onboardingError: "",
        });
      } else {
        useStore.setState({
          onboardingStep: "nostr",
          onboardingNostrDone: true,
          onboardingWalletMode: "create",
          onboardingBackupFound: false,
          onboardingError: "",
        });
      }
    } else {
      const onSubPage =
        passwordStep || verifyStep || !!mnemonic || walletMode === "restore";
      if (onSubPage) {
        useStore.setState({
          onboardingWalletMode: "create",
          onboardingWalletMnemonic: "",
          onboardingWalletPassword: "",
          onboardingWalletPasswordConfirm: "",
          onboardingWalletPasswordStep: false,
          onboardingMnemonicVerifyStep: false,
          onboardingMnemonicVerifyIndices: [],
          onboardingMnemonicVerifyInputs: [],
          onboardingBackupFound: false,
          onboardingError: "",
        });
      } else {
        // Back from "Set up your wallet" -> "Set up your identity"
        useStore.setState({
          onboardingStep: "nostr",
          onboardingNostrDone: false,
          onboardingNostrMode: "generate",
          onboardingError: "",
        });
      }
    }
  }, [passwordStep, walletMode, walletOnly, verifyStep, mnemonic]);

  // ── Wallet continue (create / restore / nostr-restore) ────────────
  const handleWalletContinue = useCallback(async () => {
    if (walletMode === "restore" && !mnemonic.trim()) {
      useStore.setState({
        onboardingError: "Please enter your recovery phrase.",
      });
      return;
    }
    if (walletMode === "create") {
      // Generate mnemonic, then show backup screen
      useStore.setState({
        onboardingLoading: true,
        onboardingError: "",
      });
      try {
        const newMnemonic = await invoke<string>("generate_mnemonic");
        const wordCount = newMnemonic.trim().split(/\s+/).length;
        const pool = Array.from({ length: wordCount }, (_, i) => i);
        for (let i = pool.length - 1; i > 0; i--) {
          const j = Math.floor(Math.random() * (i + 1));
          [pool[i], pool[j]] = [pool[j], pool[i]];
        }
        useStore.setState({
          onboardingWalletMnemonic: newMnemonic,
          onboardingMnemonicVerifyIndices: pool
            .slice(0, 3)
            .sort((a, b) => a - b),
          onboardingMnemonicVerifyInputs: ["", "", ""],
          onboardingLoading: false,
        });
      } catch (e) {
        useStore.setState({
          onboardingError: String(e),
          onboardingLoading: false,
        });
      }
      return;
    }
    // For restore/nostr-restore: go to password step
    useStore.setState({
      onboardingWalletPasswordStep: true,
      onboardingError: "",
    });
  }, [walletMode, mnemonic]);

  // ── Create wallet (after password) ────────────────────────────────
  const handleCreateWallet = useCallback(async () => {
    if (!password || password !== passwordConfirm) {
      useStore.setState({
        onboardingError: !password
          ? "Password is required."
          : "Passwords do not match.",
        onboardingWalletPassword: "",
        onboardingWalletPasswordConfirm: "",
      });
      return;
    }
    if (password.length < 8) {
      useStore.setState({
        onboardingError: "Password must be at least 8 characters.",
      });
      return;
    }
    useStore.setState({ onboardingLoading: true, onboardingError: "" });
    try {
      await invoke("restore_wallet", {
        mnemonic,
        password,
      });
      await invoke("unlock_wallet", { password });
      showToast("Wallet created!", "success");
      useStore.setState({ onboardingLoading: false });
      await finishOnboarding(queryClient);
    } catch (e) {
      useStore.setState({
        onboardingError: String(e),
        onboardingLoading: false,
      });
    }
  }, [password, passwordConfirm, mnemonic, queryClient]);

  // ── Restore wallet from seed (after password) ─────────────────────
  const handleRestoreWallet = useCallback(async () => {
    if (!mnemonic.trim() || !password || password !== passwordConfirm) {
      useStore.setState({
        onboardingError:
          !mnemonic.trim() || !password
            ? "Recovery phrase and password are required."
            : "Passwords do not match.",
        onboardingWalletPassword: "",
        onboardingWalletPasswordConfirm: "",
      });
      return;
    }
    if (password.length < 8) {
      useStore.setState({
        onboardingError: "Password must be at least 8 characters.",
      });
      return;
    }
    useStore.setState({ onboardingLoading: true, onboardingError: "" });
    try {
      await invoke("restore_wallet", {
        mnemonic: mnemonic.trim(),
        password,
      });
      await invoke("unlock_wallet", { password });
      showToast("Wallet restored!", "success");
      await finishOnboarding(queryClient);
    } catch (e) {
      useStore.setState({
        onboardingError: String(e),
        onboardingLoading: false,
      });
    }
  }, [mnemonic, password, passwordConfirm, queryClient]);

  // ── Restore from nostr backup (after password) ────────────────────
  const handleNostrRestoreWallet = useCallback(async () => {
    if (!password || password !== passwordConfirm) {
      useStore.setState({
        onboardingError: !password
          ? "Password is required."
          : "Passwords do not match.",
        onboardingWalletPassword: "",
        onboardingWalletPasswordConfirm: "",
      });
      return;
    }
    if (password.length < 8) {
      useStore.setState({
        onboardingError: "Password must be at least 8 characters.",
      });
      return;
    }
    useStore.setState({ onboardingLoading: true, onboardingError: "" });
    try {
      const selectedWallet = backupStatus?.wallets.find(
        (w) => w.d_tag === selectedDTag,
      );
      const restoredMnemonic = await invoke<string>(
        "restore_mnemonic_from_nostr",
        { walletName: selectedWallet?.name ?? "My Wallet" },
      );
      await invoke("restore_wallet", {
        mnemonic: restoredMnemonic.trim(),
        password,
      });
      await invoke("unlock_wallet", { password });
      showToast("Wallet restored from Nostr backup!", "success");
      await finishOnboarding(queryClient);
    } catch (e) {
      useStore.setState({
        onboardingError: String(e),
        onboardingLoading: false,
      });
    }
  }, [password, passwordConfirm, backupStatus, selectedDTag, queryClient]);

  // ── Copy mnemonic ─────────────────────────────────────────────────
  const handleCopyMnemonic = useCallback(() => {
    if (mnemonic) {
      void navigator.clipboard.writeText(mnemonic);
      showToast("Copied recovery phrase to clipboard");
    }
  }, [mnemonic]);

  // ── Mnemonic done -> verify step ──────────────────────────────────
  const handleMnemonicDone = useCallback(() => {
    useStore.setState({
      onboardingMnemonicVerifyStep: true,
      onboardingError: "",
    });
  }, []);

  // ── Verify mnemonic ───────────────────────────────────────────────
  const handleVerifyMnemonic = useCallback(() => {
    const words = mnemonic.trim().split(/\s+/);
    const allVerified =
      verifyIndices.length === 3 &&
      verifyIndices.every(
        (wordIdx, i) =>
          (verifyInputs[i] ?? "").trim().toLowerCase() === words[wordIdx],
      );
    if (!allVerified) {
      useStore.setState({
        onboardingError:
          "One or more words are incorrect. Check your recovery phrase and try again.",
      });
      return;
    }
    useStore.setState({
      onboardingWalletPasswordStep: true,
      onboardingMnemonicVerifyStep: false,
      onboardingError: "",
    });
  }, [mnemonic, verifyIndices, verifyInputs]);

  // ── Verify input change ───────────────────────────────────────────
  const handleVerifyInputChange = useCallback(
    (index: number, value: string) => {
      useStore.setState((s) => {
        const inputs = [...s.onboardingMnemonicVerifyInputs];
        inputs[index] = value;
        return { onboardingMnemonicVerifyInputs: inputs };
      });
    },
    [],
  );

  // ── Scanning state ────────────────────────────────────────────────
  if (backupScanning) {
    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        {stepIndicator}
        <div className="flex flex-col items-center justify-center py-8 text-center">
          <div className="h-10 w-10 animate-spin rounded-full border-2 border-slate-700 border-t-emerald-400 mb-6" />
          <p className="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">
            Please wait
          </p>
          <h2 className="text-2xl font-semibold text-white">
            Checking for backups
          </h2>
          <p className="mt-3 text-sm text-slate-400 leading-relaxed max-w-xs">
            Scanning relays for an existing encrypted wallet backup.
          </p>
        </div>
      </div>
    );
  }

  // ── Password page ─────────────────────────────────────────────────
  if (passwordStep) {
    const submitAction =
      walletMode === "create"
        ? handleCreateWallet
        : walletMode === "restore"
          ? handleRestoreWallet
          : handleNostrRestoreWallet;
    const submitLabel = loading
      ? walletMode === "create"
        ? "Creating..."
        : "Restoring..."
      : "Create password";

    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        {stepIndicator}
        <BackButton onClick={handleBack} />
        <p className="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">
          Protect your wallet
        </p>
        <h2 className="text-2xl font-semibold text-white">Set a password</h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          This password encrypts your wallet on this device. If lost, you'll
          need to restore from a backup.
        </p>
        {errorHtml && <div className="mt-5">{errorHtml}</div>}
        <div className="mt-8 space-y-6">
          <PasswordFields
            password={password}
            confirm={passwordConfirm}
            revealed={passwordRevealed}
            disabled={loading}
            onPasswordChange={(val) =>
              useStore.setState({ onboardingWalletPassword: val })
            }
            onConfirmChange={(val) =>
              useStore.setState({ onboardingWalletPasswordConfirm: val })
            }
            onToggleReveal={() =>
              useStore.setState((s) => ({
                onboardingPasswordRevealed: !s.onboardingPasswordRevealed,
              }))
            }
          />
          <button
            type="button"
            onClick={submitAction}
            disabled={loading}
            className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed transition"
          >
            {submitLabel}
          </button>
        </div>
      </div>
    );
  }

  // ── Mnemonic verify page ──────────────────────────────────────────
  if (mnemonic && walletMode === "create" && verifyStep) {
    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        {stepIndicator}
        <BackButton onClick={handleBack} />
        <p className="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">
          Verify backup
        </p>
        <h2 className="text-2xl font-semibold text-white">
          Confirm your recovery phrase
        </h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          Enter the 3 words below to confirm you've written them down correctly.
        </p>
        <div className="mt-8 space-y-4">
          {verifyIndices.map((wordIdx, i) => (
            <div
              key={`verify-word-${wordIdx}`}
              className="flex items-center gap-4"
            >
              <span className="w-16 shrink-0 text-right text-xs font-medium text-slate-500">
                Word {wordIdx + 1}
              </span>
              <input
                type="text"
                placeholder={`type word ${wordIdx + 1}`}
                autoComplete="off"
                spellCheck={false}
                value={verifyInputs[i] ?? ""}
                onChange={(e) => handleVerifyInputChange(i, e.target.value)}
                className="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 transition focus:ring-2 mono"
              />
            </div>
          ))}
        </div>
        {errorHtml && <div className="mt-5">{errorHtml}</div>}
        <button
          type="button"
          onClick={handleVerifyMnemonic}
          className="mt-8 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition"
        >
          Confirm
        </button>
      </div>
    );
  }

  // ── Mnemonic display page ─────────────────────────────────────────
  if (mnemonic && walletMode === "create") {
    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        {stepIndicator}
        <BackButton onClick={handleBack} />
        <p className="text-xs font-semibold uppercase tracking-widest text-emerald-400 mb-2">
          Wallet created
        </p>
        <h2 className="text-2xl font-semibold text-white">
          Save your recovery phrase
        </h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          Write these 12 words down in order and store them somewhere safe. This
          is the only way to recover your wallet.
        </p>
        <div className="mt-6 rounded-xl border border-slate-700 bg-slate-900/60 p-5 space-y-4">
          <MnemonicGrid mnemonic={mnemonic} />
          <button
            type="button"
            onClick={handleCopyMnemonic}
            className="w-full rounded-lg border border-slate-700 px-4 py-2.5 text-sm text-slate-300 hover:bg-slate-800 transition"
          >
            Copy to clipboard
          </button>
        </div>
        <button
          type="button"
          onClick={handleMnemonicDone}
          className="mt-8 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition"
        >
          I've saved my recovery phrase
        </button>
      </div>
    );
  }

  // ── Restore from seed sub-page ────────────────────────────────────
  if (walletMode === "restore") {
    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        {stepIndicator}
        <BackButton onClick={handleBack} />
        {!walletOnly && (
          <p className="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">
            Step 2 of 2
          </p>
        )}
        <h2 className="text-2xl font-semibold text-white">Restore from seed</h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          Enter your 12-word recovery phrase to restore your existing wallet.
        </p>
        {errorHtml && <div className="mt-5">{errorHtml}</div>}
        <div className="mt-8 space-y-4">
          <div className="space-y-1.5">
            <label
              htmlFor="wallet-recovery-phrase"
              className="text-xs font-medium text-slate-400 uppercase tracking-wide"
            >
              Recovery Phrase
            </label>
            <textarea
              id="wallet-recovery-phrase"
              placeholder="word1 word2 word3 ... word12"
              rows={3}
              value={mnemonic}
              onChange={(e) =>
                useStore.setState({
                  onboardingWalletMnemonic: e.target.value,
                })
              }
              className="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 transition focus:ring-2 resize-none mono"
            />
          </div>
          <button
            type="button"
            onClick={handleWalletContinue}
            className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition"
          >
            Restore wallet
          </button>
        </div>
      </div>
    );
  }

  // ── Nostr restore sub-page ────────────────────────────────────────
  if (walletMode === "nostr-restore") {
    const wallets = backupStatus?.wallets ?? [];
    const effectiveSelectedDTag = selectedDTag || (wallets[0]?.d_tag ?? "");
    const backupRelays =
      backupStatus?.relay_results?.filter((r) => r.has_backup) ?? [];
    const fallbackRelay =
      relays[0]?.url ?? backupStatus?.relay_results?.[0]?.url ?? "";
    const primaryRelayUrl =
      backupRelays.length > 0 ? backupRelays[0].url : fallbackRelay;
    const primaryRelay = primaryRelayUrl
      .replace(/^wss?:\/\//, "")
      .replace(/\/$/, "");
    const othersCount = backupRelays.length > 1 ? backupRelays.length - 1 : 0;

    const walletsToRender =
      wallets.length > 0 ? wallets : [{ name: "My Wallet", d_tag: "" }];

    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        {stepIndicator}
        <p className="text-xs font-semibold uppercase tracking-widest text-emerald-400 mb-2">
          Backup found
        </p>
        <h2 className="text-2xl font-semibold text-white">
          Restore from Nostr backup
        </h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          Select a wallet to restore.
        </p>
        <div className="mt-6 space-y-2">
          {walletsToRender.map((w) => {
            const isSelected =
              w.d_tag === effectiveSelectedDTag || wallets.length === 1;
            return (
              <button
                type="button"
                key={w.d_tag}
                onClick={() =>
                  useStore.setState({
                    onboardingSelectedWalletDTag: w.d_tag,
                  })
                }
                className={`w-full flex items-center justify-between rounded-xl border ${
                  isSelected
                    ? "border-emerald-600/50 bg-emerald-950/20"
                    : "border-slate-700 bg-slate-900/40 hover:border-slate-600"
                } px-4 py-3.5 transition text-left`}
              >
                <div className="flex items-center gap-3 min-w-0">
                  <svg
                    className={`h-4 w-4 shrink-0 ${isSelected ? "text-emerald-400" : "text-slate-500"}`}
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeWidth={1.5}
                    aria-hidden="true"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M21 12a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 12m18 0v6a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 18v-6m18 0V9M3 12V9m18-3a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 9m18 0V9a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 9"
                    />
                  </svg>
                  <div className="min-w-0">
                    <p
                      className={`text-sm font-medium ${isSelected ? "text-emerald-300" : "text-slate-300"} truncate`}
                    >
                      {w.name}
                    </p>
                    <p className="text-xs text-slate-500 mono truncate">
                      {primaryRelay}
                      {othersCount > 0 && (
                        <span className="not-mono">
                          {" "}
                          and {othersCount} other
                          {othersCount > 1 ? "s" : ""}
                        </span>
                      )}
                    </p>
                  </div>
                </div>
                {isSelected && (
                  <svg
                    className="h-4 w-4 text-emerald-400 shrink-0 ml-2"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeWidth={2.5}
                    aria-hidden="true"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M5 13l4 4L19 7"
                    />
                  </svg>
                )}
              </button>
            );
          })}
        </div>
        <div className="mt-6 space-y-3">
          <button
            type="button"
            onClick={handleWalletContinue}
            className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition"
          >
            Restore wallet
          </button>
          <button
            type="button"
            onClick={() =>
              useStore.setState({
                onboardingWalletMode: "create",
                onboardingBackupFound: false,
                onboardingError: "",
              })
            }
            className="w-full rounded-lg border border-slate-700 px-4 py-3.5 text-sm font-medium text-slate-300 hover:bg-slate-800 hover:border-slate-600 transition"
          >
            Set up a new wallet
          </button>
        </div>
      </div>
    );
  }

  // ── Main wallet setup page ────────────────────────────────────────
  const backupFoundWallets = backupStatus?.wallets ?? [];
  const backupFoundCard = backupFound ? (
    <button
      type="button"
      onClick={() =>
        useStore.setState({
          onboardingWalletMode: "nostr-restore",
          onboardingError: "",
        })
      }
      className="w-full rounded-xl border border-emerald-700/40 bg-emerald-950/20 hover:border-emerald-600/50 p-4 text-left transition"
    >
      <div className="flex items-start gap-3">
        <svg
          className="h-5 w-5 text-emerald-400 shrink-0 mt-0.5"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={1.5}
          aria-hidden="true"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M12 16.5V9.75m0 0l3 3m-3-3l-3 3M6.75 19.5a4.5 4.5 0 01-1.41-8.775 5.25 5.25 0 0110.233-2.33 3 3 0 013.758 3.848A3.752 3.752 0 0118 19.5H6.75z"
          />
        </svg>
        <div>
          <p className="text-sm font-medium text-emerald-300">
            {backupFoundWallets.length === 1
              ? backupFoundWallets[0].name
              : `${backupFoundWallets.length} wallet backup${backupFoundWallets.length !== 1 ? "s" : ""} found`}
          </p>
          <p className="mt-1 text-xs text-emerald-400/60 leading-relaxed">
            Restore your existing wallet from an encrypted Nostr backup
          </p>
        </div>
      </div>
    </button>
  ) : null;

  return (
    <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
      {stepIndicator}
      {!walletOnly && !backupScanning && <BackButton onClick={handleBack} />}
      {!walletOnly && (
        <p className="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">
          Step 2 of 2
        </p>
      )}
      <h2 className="text-2xl font-semibold text-white">Set up your wallet</h2>
      <p className="mt-3 text-sm text-slate-400 leading-relaxed">
        Create a new Liquid wallet or restore an existing one.
      </p>
      {errorHtml && <div className="mt-5">{errorHtml}</div>}
      {backupFoundCard && <div className="mt-5">{backupFoundCard}</div>}
      <div className="mt-10 space-y-3">
        <button
          type="button"
          onClick={handleWalletContinue}
          className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition"
        >
          Create new wallet
        </button>
        <button
          type="button"
          onClick={() =>
            useStore.setState({
              onboardingWalletMode: "restore",
              onboardingError: "",
            })
          }
          className="w-full rounded-lg border border-slate-700 px-4 py-3.5 text-sm font-medium text-slate-300 hover:bg-slate-800 hover:border-slate-600 transition"
        >
          Restore from seed
        </button>
      </div>
    </div>
  );
}
