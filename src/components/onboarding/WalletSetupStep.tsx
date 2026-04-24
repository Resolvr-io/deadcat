import { useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { type ReactNode, useCallback, useEffect, useState } from "react";
import { setWalletNeedsBackup } from "../../hooks/useWalletNeedsBackup";
import { useStore } from "../../store";
import type { NostrProfile } from "../../types";
import { generateAvatarDataUri } from "../../utils-react/avatar";
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

// ── Password fields ─────────────────────────────────────────────────
export function PasswordFields({
  password,
  confirm,
  revealed,
  disabled,
  label = "Password",
  onPasswordChange,
  onConfirmChange,
  onToggleReveal,
  onSubmit,
}: {
  password: string;
  confirm: string;
  revealed: boolean;
  disabled: boolean;
  label?: string;
  onPasswordChange: (val: string) => void;
  onConfirmChange: (val: string) => void;
  onToggleReveal: () => void;
  /** Fires when the user presses Enter in either password input and
   *  the passwords are non-empty, long enough, and match. Callers
   *  still need to do their own side-effect guards (loading state,
   *  additional validation like mnemonic present). */
  onSubmit?: () => void;
}) {
  const inputType = revealed ? "text" : "password";
  const canSubmit =
    !disabled && !!password && password.length >= 8 && password === confirm;
  const handleEnter = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== "Enter") return;
    if (!onSubmit || !canSubmit) return;
    e.preventDefault();
    onSubmit();
  };

  return (
    <div className="space-y-4">
      <div className="space-y-1.5">
        <label
          htmlFor="wallet-password"
          className="text-xs font-medium text-slate-400 uppercase tracking-wide"
        >
          {label}
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
            onKeyDown={handleEnter}
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
          onKeyDown={handleEnter}
          disabled={disabled}
          className={`h-11 w-full rounded-lg border ${confirm && password !== confirm ? "border-red-500/50" : "border-slate-700"} bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50`}
        />
      </div>
      {/* Reserved-height validation row so the layout doesn't jump as
          the user types. Mirrors the generate-flow profile-step UX. */}
      <div className="-mt-2 h-5">
        {password && password.length < 8 ? (
          <p className="text-xs text-amber-300">Minimum 8 characters</p>
        ) : confirm && password !== confirm ? (
          <p className="text-xs text-rose-400">Passwords don&apos;t match</p>
        ) : null}
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
  const backupScanning = useStore((s) => s.onboardingBackupScanning);
  const backupFound = useStore((s) => s.onboardingBackupFound);
  const backupStatus = useStore((s) => s.nostrBackupStatus);
  const selectedDTag = useStore((s) => s.onboardingSelectedWalletDTag);
  const relays = useStore((s) => s.relays);
  const nostrMode = useStore((s) => s.onboardingNostrMode);
  const nostrProfile = useStore((s) => s.nostrProfile);
  const nostrNpub = useStore((s) => s.nostrNpub);
  const queryClient = useQueryClient();

  // Clear any leftover password state every time this step mounts.
  // The modal's close handler already wipes these, but certain paths
  // (backdrop dismiss, race with wallet-state transitions, stale
  // zustand entries) have left a previous session's confirm value
  // in place, producing a spurious "Passwords don't match" on a
  // fresh open. Belt-and-suspenders: zero-out on every mount.
  useEffect(() => {
    useStore.setState({
      onboardingWalletPassword: "",
      onboardingWalletPasswordConfirm: "",
    });
  }, []);

  // Wallet choice inside the bunker setup screen: create a new wallet
  // or restore an existing one from its recovery phrase. Mirrors the
  // same toggle in the nsec-restore flow for UX parity.
  const [bunkerWalletChoice, setBunkerWalletChoice] = useState<
    "new" | "restore"
  >("new");
  // Tracks whether the profile fetch has settled at least once (success
  // or failure). Used to stop showing "Loading profile…" indefinitely.
  const [profileAttempted, setProfileAttempted] = useState(false);

  // Fetch the user's kind:0 profile for the bunker identity card.
  // Uses `preview_nostr_profile` (2s connect + 3s fetch, standalone
  // client) rather than the node-backed `fetch_nostr_profile` which
  // has a 10s timeout and depends on the node's relay pool being
  // fully connected.
  useEffect(() => {
    if (nostrMode !== "bunker") return;
    if (!nostrNpub) return;
    if (nostrProfile) {
      setProfileAttempted(true);
      return;
    }
    let cancelled = false;
    void (async () => {
      // Try the fast standalone client first. If relays were slow or the
      // kind:0 wasn't on our defaults, retry a couple of times, then fall
      // back to the node-backed fetch (its relay pool is already warm
      // from the NIP-46 subscription).
      const attempts: Array<() => Promise<NostrProfile | null>> = [
        () =>
          invoke<NostrProfile | null>("preview_nostr_profile", {
            npub: nostrNpub,
          }),
        () =>
          invoke<NostrProfile | null>("preview_nostr_profile", {
            npub: nostrNpub,
          }),
        () => invoke<NostrProfile | null>("fetch_nostr_profile"),
      ];
      for (const attempt of attempts) {
        if (cancelled) return;
        try {
          const profile = await attempt();
          if (cancelled) return;
          if (profile) {
            useStore.setState({ nostrProfile: profile });
            setProfileAttempted(true);
            return;
          }
        } catch {
          // Try the next strategy
        }
        // Small gap between attempts so relays have a moment to settle
        await new Promise((r) => setTimeout(r, 800));
      }
      if (!cancelled) setProfileAttempted(true);
    })();
    return () => {
      cancelled = true;
    };
  }, [nostrMode, nostrNpub, nostrProfile]);

  const errorHtml = error ? (
    <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3">
      <p className="text-sm text-red-400">{error}</p>
    </div>
  ) : null;

  // ── Back handler ──────────────────────────────────────────────────
  const handleBack = useCallback(() => {
    useStore.setState({ onboardingPasswordRevealed: false });

    if (
      passwordStep &&
      (walletMode === "create" ||
        walletMode === "restore" ||
        walletMode === "nostr-restore")
    ) {
      // Back from password page -> prior screen (main wallet page or restore sub-page)
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
      const onSubPage = passwordStep || !!mnemonic || walletMode === "restore";
      if (onSubPage) {
        useStore.setState({
          onboardingWalletMode: "create",
          onboardingWalletMnemonic: "",
          onboardingWalletPassword: "",
          onboardingWalletPasswordConfirm: "",
          onboardingWalletPasswordStep: false,
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
  }, [passwordStep, walletMode, walletOnly, mnemonic]);

  // ── Wallet continue (create / restore / nostr-restore) ────────────
  const handleWalletContinue = useCallback(async () => {
    if (walletMode === "restore" && !mnemonic.trim()) {
      useStore.setState({
        onboardingError: "Please enter your recovery phrase.",
      });
      return;
    }
    // All modes: go straight to password step. For "create" the mnemonic is
    // generated atomically inside `create_wallet(password)` at commit time,
    // so there's no separate "save your recovery phrase" / verify step.
    // Users can always retrieve the mnemonic later from Settings.
    useStore.setState({
      onboardingWalletPasswordStep: true,
      onboardingWalletPassword: "",
      onboardingWalletPasswordConfirm: "",
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
      // Backend generates the mnemonic, encrypts it with the password, and
      // persists it in one shot. User never sees the raw mnemonic during
      // onboarding — it's retrievable later from Settings.
      await invoke("create_wallet", { password });
      setWalletNeedsBackup(true);
      await invoke("unlock_wallet", { password });
      // The `app_state_updated` event listener only syncs
      // `unlocked → locked` transitions to the store; we must
      // explicitly flip walletStatus here so finishOnboarding's
      // `walletOpen: true` doesn't land on WalletPage's legacy
      // setup screen with a stale "not_created" status.
      useStore.setState({
        walletStatus: "unlocked",
        walletSessionPassword: password,
        onboardingLoading: false,
      });
      showToast("Wallet created!", "success");
      await finishOnboarding(queryClient);
    } catch (e) {
      useStore.setState({
        onboardingError: String(e),
        onboardingLoading: false,
      });
    }
  }, [password, passwordConfirm, queryClient]);

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
      setWalletNeedsBackup(false);
      await invoke("unlock_wallet", { password });
      useStore.setState({
        walletStatus: "unlocked",
        walletSessionPassword: password,
        onboardingLoading: false,
      });
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
      setWalletNeedsBackup(false);
      await invoke("unlock_wallet", { password });
      useStore.setState({
        walletStatus: "unlocked",
        walletSessionPassword: password,
        onboardingLoading: false,
      });
      showToast("Wallet restored from Nostr backup!", "success");
      await finishOnboarding(queryClient);
    } catch (e) {
      useStore.setState({
        onboardingError: String(e),
        onboardingLoading: false,
      });
    }
  }, [password, passwordConfirm, backupStatus, selectedDTag, queryClient]);

  // ── Combined bunker wallet commit ─────────────────────────────────
  // Single-screen flow for NIP-46 users: validate password, then either
  // restore a typed mnemonic or generate a fresh wallet.
  const handleBunkerWalletSubmit = useCallback(async () => {
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
    const typedMnemonic = mnemonic.trim();
    const restoringWallet = bunkerWalletChoice === "restore";
    if (restoringWallet && !typedMnemonic) {
      useStore.setState({
        onboardingError: "Enter your 12-word wallet recovery phrase.",
      });
      return;
    }
    useStore.setState({ onboardingLoading: true, onboardingError: "" });
    try {
      if (restoringWallet) {
        await invoke("restore_wallet", {
          mnemonic: typedMnemonic,
          password,
        });
        setWalletNeedsBackup(false);
        showToast("Wallet restored!", "success");
      } else {
        await invoke("create_wallet", { password });
        setWalletNeedsBackup(true);
        showToast("Wallet created!", "success");
      }
      await invoke("unlock_wallet", { password });
      // The `app_state_updated` event listener only syncs `unlocked → locked`
      // transitions to the store; we must explicitly flip the store's
      // walletStatus here so WalletPage doesn't land on its legacy setup
      // screen after onboarding finishes.
      useStore.setState({
        walletStatus: "unlocked",
        walletSessionPassword: password,
        onboardingLoading: false,
      });
      await finishOnboarding(queryClient);
    } catch (e) {
      useStore.setState({
        onboardingError: String(e),
        onboardingLoading: false,
      });
    }
  }, [password, passwordConfirm, mnemonic, bunkerWalletChoice, queryClient]);

  // ── Combined bunker setup screen ──────────────────────────────────
  // Single page that shows the connected Nostr identity, an optional
  // recovery-phrase input, password fields, and a single submit button.
  // Replaces the legacy multi-step Confirm Identity → Set up wallet flow.
  if (nostrMode === "bunker") {
    const displayName =
      nostrProfile?.display_name || nostrProfile?.name || "Nostr identity";
    const avatarSrc = nostrProfile?.picture
      ? nostrProfile.picture
      : nostrNpub
        ? generateAvatarDataUri(nostrNpub)
        : "";
    const nip05 = nostrProfile?.nip05 ?? "";
    const truncNpub = nostrNpub
      ? `${nostrNpub.slice(0, 12)}...${nostrNpub.slice(-6)}`
      : "";
    // Show real profile once fetched; fall back to npub after the
    // first attempt settles even if nothing was found.
    const profileReady = profileAttempted;
    const restoringWallet = bunkerWalletChoice === "restore";
    const submitLabel = loading
      ? "Creating..."
      : restoringWallet
        ? "Restore wallet"
        : "Create wallet";

    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        <BackButton onClick={handleBack} />
        {stepIndicator}

        {/* Identity card — positioned above the title so the password
            fields below are unambiguously scoped to the wallet, not the
            signer-held Nostr identity. */}
        <p className="mb-2 text-xs font-semibold uppercase tracking-widest text-emerald-400">
          Signed in as
        </p>
        <div className="mb-6 flex items-center gap-3 rounded-xl border border-slate-700 bg-slate-900/60 p-4">
          {avatarSrc ? (
            <img
              src={avatarSrc}
              alt=""
              className="h-10 w-10 shrink-0 rounded-full object-cover"
            />
          ) : (
            <div className="h-10 w-10 shrink-0 rounded-full bg-slate-800" />
          )}
          <div className="min-w-0 flex-1">
            {profileReady ? (
              <>
                <p className="truncate text-sm font-semibold text-slate-100">
                  {displayName}
                </p>
                <p className="mono truncate text-xs text-slate-500">
                  {nip05 || truncNpub}
                </p>
              </>
            ) : (
              <>
                <p className="flex items-center gap-2 text-sm font-semibold text-slate-400">
                  <span className="h-3 w-3 animate-spin rounded-full border-2 border-slate-700 border-t-emerald-400" />
                  Loading profile…
                </p>
                <p className="mono mt-0.5 truncate text-xs text-slate-600">
                  {truncNpub}
                </p>
              </>
            )}
          </div>
        </div>

        <h2 className="text-2xl font-semibold text-white">
          Set up your wallet
        </h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          Deadcat uses a self-custodial wallet on Bitcoin's Liquid Network,
          separate from your remote signer. Your password encrypts it on this
          device.
        </p>

        {errorHtml && <div className="mt-5">{errorHtml}</div>}

        {/* Wallet choice */}
        <div className="mt-6">
          <p className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-slate-400">
            Wallet
          </p>
          <div className="grid grid-cols-2 gap-2 rounded-lg border border-slate-700 bg-slate-900 p-1">
            <button
              type="button"
              onClick={() => {
                setBunkerWalletChoice("new");
                useStore.setState({ onboardingWalletMnemonic: "" });
              }}
              disabled={loading}
              className={`rounded-md px-3 py-2 text-xs font-medium transition ${
                bunkerWalletChoice === "new"
                  ? "bg-emerald-400 text-slate-950"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              Create new
            </button>
            <button
              type="button"
              onClick={() => setBunkerWalletChoice("restore")}
              disabled={loading}
              className={`rounded-md px-3 py-2 text-xs font-medium transition ${
                bunkerWalletChoice === "restore"
                  ? "bg-emerald-400 text-slate-950"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              Restore from phrase
            </button>
          </div>
          <p className="mt-1.5 text-xs text-slate-600">
            {restoringWallet
              ? "We'll restore your existing wallet from its 12-word phrase."
              : "A new wallet will be created and secured by the password below."}
          </p>
        </div>

        {restoringWallet && (
          <div className="mt-5">
            <label
              htmlFor="bunker-recovery-phrase"
              className="mb-1.5 block text-xs font-medium uppercase tracking-wide text-slate-400"
            >
              Wallet Recovery Phrase
            </label>
            <textarea
              id="bunker-recovery-phrase"
              rows={3}
              placeholder="Enter your 12-word phrase..."
              value={mnemonic}
              onChange={(e) =>
                useStore.setState({
                  onboardingWalletMnemonic: e.target.value,
                })
              }
              className="mono w-full rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-sm outline-none ring-emerald-400 transition focus:ring-2"
              disabled={loading}
            />
          </div>
        )}

        {/* Password */}
        <div className="mt-5">
          <PasswordFields
            password={password}
            confirm={passwordConfirm}
            revealed={passwordRevealed}
            disabled={loading}
            label="Wallet password"
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
            onSubmit={handleBunkerWalletSubmit}
          />
        </div>

        <button
          type="button"
          onClick={handleBunkerWalletSubmit}
          disabled={loading}
          className="mt-6 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 transition hover:bg-emerald-300 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {submitLabel}
        </button>
      </div>
    );
  }

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
        <BackButton onClick={handleBack} />
        {stepIndicator}
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
            onSubmit={submitAction}
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

  // ── Restore from seed sub-page ────────────────────────────────────
  if (walletMode === "restore") {
    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        <BackButton onClick={handleBack} />
        {stepIndicator}
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
          onboardingWalletPassword: "",
          onboardingWalletPasswordConfirm: "",
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
      {!walletOnly && !backupScanning && <BackButton onClick={handleBack} />}
      {stepIndicator}
      {!walletOnly && (
        <p className="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">
          Step 2 of 2
        </p>
      )}
      <h2 className="text-2xl font-semibold text-white">Set up your wallet</h2>
      <p className="mt-3 text-sm text-slate-400 leading-relaxed">
        Deadcat uses a self-custodial wallet on Bitcoin's Liquid Network. Create
        a new wallet or restore an existing one.
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
              onboardingWalletPassword: "",
              onboardingWalletPasswordConfirm: "",
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
