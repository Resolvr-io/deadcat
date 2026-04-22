import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useState } from "react";
import { baseCurrencyOptions } from "../../constants";
import { useLockScroll } from "../../hooks/useLockScroll";
import { queryClient } from "../../queries/queryClient";
import { useStore } from "../../store";
import type { BaseCurrency, Nip46Status, RelayEntry } from "../../types";
import { randomCatName } from "../../utils-react/random-name";
import { btcLabel } from "../../utils-react/wallet";
import { CloseButton } from "../shared/CloseButton";
import { showToast } from "../shared/Toast";

const DEV_MODE = import.meta.env.DEV;

/* ── Accordion section ─────────────────────────────────────────────── */

function SettingsAccordion({
  sectionKey,
  title,
  children,
}: {
  sectionKey: string;
  title: string;
  children: React.ReactNode;
}) {
  const open = useStore((s) => s.settingsSection[sectionKey]);

  const toggle = useCallback(() => {
    useStore.setState((s) => ({
      settingsSection: {
        ...s.settingsSection,
        [sectionKey]: !s.settingsSection[sectionKey],
      },
    }));
  }, [sectionKey]);

  return (
    <div>
      <button
        type="button"
        onClick={toggle}
        className="w-full flex items-center justify-between py-3 text-left"
      >
        <span className="text-sm font-semibold text-slate-200">{title}</span>
        <svg
          aria-hidden="true"
          className={`h-4 w-4 text-slate-500 transition-transform duration-200 ${open ? "rotate-180" : ""}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth="2"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>
      {open && <div className="pb-4">{children}</div>}
      <div className="border-b border-slate-800" />
    </div>
  );
}

/* ── Toggle switch helper ──────────────────────────────────────────── */

function Toggle({
  enabled,
  onClick,
}: {
  enabled: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`relative h-5 w-9 rounded-full transition ${enabled ? "bg-emerald-400" : "bg-slate-700"}`}
    >
      <span
        className={`absolute top-0.5 ${enabled ? "left-[18px]" : "left-0.5"} h-4 w-4 rounded-full bg-white shadow transition-all`}
      />
    </button>
  );
}

/* ── Nostr Identity section ────────────────────────────────────────── */

function NostrSection() {
  const nostrNpub = useStore((s) => s.nostrNpub);
  const nostrNsecRevealed = useStore((s) => s.nostrNsecRevealed);
  const walletStatus = useStore((s) => s.walletStatus);
  const [nsecPasswordInput, setNsecPasswordInput] = useState("");
  const [nsecPasswordPrompt, setNsecPasswordPrompt] = useState(false);
  const [nsecPasswordError, setNsecPasswordError] = useState("");
  const [nip46Status, setNip46Status] = useState<Nip46Status | null>(null);

  // Auto-hide nsec when wallet locks
  useEffect(() => {
    if (walletStatus !== "unlocked") {
      useStore.setState({ nostrNsecRevealed: null });
      setNsecPasswordPrompt(false);
      setNsecPasswordInput("");
      setNsecPasswordError("");
    }
  }, [walletStatus]);

  useEffect(() => {
    invoke<Nip46Status | null>("get_nip46_status")
      .then((status) => setNip46Status(status))
      .catch(() => {});
  }, []);

  const isRemoteSigner = nip46Status?.connected === true;

  const copyNpub = useCallback(async () => {
    const npub = useStore.getState().nostrNpub;
    if (npub) {
      await navigator.clipboard.writeText(npub);
      showToast("Copied npub to clipboard");
    }
  }, []);

  const copyNsec = useCallback(async () => {
    const nsec = useStore.getState().nostrNsecRevealed;
    if (nsec) {
      await navigator.clipboard.writeText(nsec);
      useStore.setState({ nostrNsecRevealed: null });
      setNsecPasswordPrompt(false);
      setNsecPasswordInput("");
      showToast("Copied nsec to clipboard");
    }
  }, []);

  const handleRevealClick = useCallback(() => {
    // If wallet is unlocked with a session password, use it directly
    const sessionPw = useStore.getState().walletSessionPassword;
    if (walletStatus === "unlocked" && sessionPw) {
      void (async () => {
        try {
          // Verify the session password is valid
          await invoke<string>("get_wallet_mnemonic", {
            password: sessionPw,
          });
          const nsec = await invoke<string>("export_nostr_nsec");
          useStore.setState({ nostrNsecRevealed: nsec });
        } catch {
          // Session password invalid or wallet issue — fall back to prompt
          setNsecPasswordPrompt(true);
          setNsecPasswordError("");
        }
      })();
    } else {
      setNsecPasswordPrompt(true);
      setNsecPasswordError("");
    }
  }, [walletStatus]);

  const handlePasswordSubmit = useCallback(async () => {
    if (!nsecPasswordInput) return;
    try {
      await invoke<string>("get_wallet_mnemonic", {
        password: nsecPasswordInput,
      });
      const nsec = await invoke<string>("export_nostr_nsec");
      useStore.setState({ nostrNsecRevealed: nsec });
      setNsecPasswordError("");
    } catch {
      setNsecPasswordError("Incorrect password");
    }
  }, [nsecPasswordInput]);

  return (
    <div className="space-y-4">
      {/* Signer type badge */}
      {nostrNpub && (
        <div className="flex items-center gap-2">
          <span
            className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[10px] font-medium ${
              isRemoteSigner
                ? "bg-blue-500/10 text-blue-300 border border-blue-500/20"
                : "bg-emerald-500/10 text-emerald-300 border border-emerald-500/20"
            }`}
          >
            <span
              className={`h-1.5 w-1.5 rounded-full ${isRemoteSigner ? "bg-blue-400" : "bg-emerald-400"}`}
            />
            {isRemoteSigner ? "Remote Signer (NIP-46)" : "Local Keys"}
          </span>
        </div>
      )}

      <p className="text-xs text-slate-500">
        {isRemoteSigner
          ? "Your keys are managed by an external signer via the NIP-46 protocol."
          : "Your Nostr keypair is your unique identity, and is used to publish markets and sign attestations."}
      </p>

      {/* npub */}
      <div>
        <p className="text-xs text-slate-500 mb-1">Public Key</p>
        <div className="flex items-center gap-2">
          <p className="mono text-xs text-slate-300 min-w-0 truncate">
            {nostrNpub ?? "Not initialized"}
          </p>
          {nostrNpub && (
            <button
              type="button"
              onClick={copyNpub}
              className="shrink-0 text-xs text-slate-500 hover:text-slate-300 transition"
            >
              <svg
                aria-hidden="true"
                className="h-3.5 w-3.5"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
              </svg>
            </button>
          )}
        </div>
      </div>

      {/* NIP-46 bunker info */}
      {isRemoteSigner && nip46Status && (
        <div className="rounded-lg border border-blue-700/20 bg-blue-950/10 px-3 py-2.5 space-y-1.5">
          <div>
            <p className="text-[10px] text-slate-500 uppercase tracking-wide">
              Relay
            </p>
            <p className="mono text-xs text-slate-400 truncate">
              {nip46Status.relayUrls[0] ?? "unknown"}
            </p>
          </div>
        </div>
      )}

      {/* nsec — only for local keys */}
      {nostrNpub && !isRemoteSigner && (
        <div>
          <p className="text-xs text-slate-500 mb-1">Secret Key</p>
          <div className="flex items-center gap-2">
            {nostrNsecRevealed ? (
              <>
                <p className="mono text-xs text-rose-300 min-w-0 truncate">
                  {nostrNsecRevealed}
                </p>
                <button
                  type="button"
                  onClick={copyNsec}
                  className="shrink-0 text-xs text-slate-500 hover:text-slate-300 transition"
                >
                  <svg
                    aria-hidden="true"
                    className="h-3.5 w-3.5"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                  </svg>
                </button>
              </>
            ) : nsecPasswordPrompt ? (
              <div className="flex flex-col gap-1.5 w-full">
                <div className="flex items-center gap-2">
                  <input
                    type="password"
                    value={nsecPasswordInput}
                    onChange={(e) => setNsecPasswordInput(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handlePasswordSubmit();
                    }}
                    placeholder="Account password"
                    className="h-7 flex-1 min-w-0 rounded border border-slate-700 bg-slate-900 px-2 text-xs outline-none ring-emerald-400 transition focus:ring-1"
                  />
                  <button
                    type="button"
                    onClick={() => void handlePasswordSubmit()}
                    className="shrink-0 rounded border border-amber-700/40 px-2.5 py-1 text-xs text-amber-300 hover:bg-amber-900/20 transition"
                  >
                    Reveal
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setNsecPasswordPrompt(false);
                      setNsecPasswordInput("");
                      setNsecPasswordError("");
                    }}
                    className="shrink-0 text-xs text-slate-500 hover:text-slate-300 transition"
                  >
                    Cancel
                  </button>
                </div>
                {nsecPasswordError && (
                  <p className="text-xs text-rose-300">{nsecPasswordError}</p>
                )}
              </div>
            ) : (
              <>
                <p className="text-xs text-slate-600 italic">
                  Hidden for your protection
                </p>
                <button
                  type="button"
                  onClick={handleRevealClick}
                  className="shrink-0 text-xs text-amber-300 hover:text-amber-200 transition"
                >
                  Reveal
                </button>
              </>
            )}
          </div>
        </div>
      )}

      {nostrNpub && !isRemoteSigner && (
        <p className="text-xs text-amber-300/70">
          Back up your nsec. You need it to resolve markets you create.
        </p>
      )}

      {!nostrNpub ? (
        <button
          type="button"
          onClick={() => useStore.setState({ nostrReplacePanel: true })}
          className="w-full rounded-lg border border-slate-700 px-4 py-2.5 text-sm text-slate-300 hover:bg-slate-800 transition"
        >
          Set up identity
        </button>
      ) : isRemoteSigner ? null : (
        <button
          type="button"
          onClick={() => useStore.setState({ nostrReplacePanel: true })}
          className="text-xs text-rose-400 hover:text-rose-300 transition"
        >
          Replace Nostr Keys
        </button>
      )}
    </div>
  );
}

/* ── Wallet section ────────────────────────────────────────────────── */

function SeedPhraseReveal() {
  const walletStatus = useStore((s) => s.walletStatus);
  const [revealed, setRevealed] = useState(false);
  const [words, setWords] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const handleReveal = useCallback(async () => {
    if (walletStatus !== "unlocked") return;
    setLoading(true);
    try {
      let mnemonic: string;
      try {
        mnemonic = await invoke<string>("get_cached_mnemonic");
      } catch {
        const pw = useStore.getState().walletSessionPassword;
        if (!pw) {
          setLoading(false);
          return;
        }
        mnemonic = await invoke<string>("get_wallet_mnemonic", {
          password: pw,
        });
      }
      setWords(mnemonic.split(" "));
      setRevealed(true);
    } catch {
      // Failed
    }
    setLoading(false);
  }, [walletStatus]);

  const handleCopy = useCallback(() => {
    if (words.length > 0) {
      void navigator.clipboard.writeText(words.join(" "));
      setRevealed(false);
      setWords([]);
      showToast("Copied recovery phrase");
    }
  }, [words]);

  return (
    <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-3 space-y-2">
      <p className="text-xs font-medium uppercase tracking-wider text-slate-500">
        Recovery Phrase
      </p>
      {revealed ? (
        <>
          <div className="rounded-lg border border-slate-700 bg-slate-950/60 p-3">
            <div className="grid grid-cols-3 gap-x-4 gap-y-1.5">
              {words.map((word, i) => (
                <div
                  key={`${i}-${word}`}
                  className="flex items-baseline gap-1.5"
                >
                  <span className="text-xs text-slate-600 shrink-0 w-4 text-right">
                    {i + 1}.
                  </span>
                  <span className="mono text-xs text-slate-200">{word}</span>
                </div>
              ))}
            </div>
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={handleCopy}
              className="flex-1 rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
            >
              Copy to clipboard
            </button>
            <button
              type="button"
              onClick={() => {
                setRevealed(false);
                setWords([]);
              }}
              className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition"
            >
              Hide
            </button>
          </div>
          <div className="flex gap-2 rounded-lg border border-amber-700/30 bg-amber-950/20 px-3 py-2">
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
            <p className="text-xs text-amber-300/80">
              Never share your recovery phrase. Anyone with it can access your
              funds.
            </p>
          </div>
        </>
      ) : (
        <>
          <p className="text-xs text-slate-400">
            Your 12-word recovery phrase is the only way to restore your wallet.
            Keep it safe and never share it.
          </p>
          <button
            type="button"
            onClick={handleReveal}
            disabled={loading || walletStatus !== "unlocked"}
            className="w-full rounded-lg border border-amber-700/40 px-4 py-2 text-xs text-amber-300 hover:bg-amber-900/20 transition disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {walletStatus !== "unlocked"
              ? "Unlock wallet to reveal"
              : loading
                ? "Decrypting..."
                : "Reveal Recovery Phrase"}
          </button>
        </>
      )}
    </div>
  );
}

function InlineUnlock() {
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [unlocking, setUnlocking] = useState(false);

  const handleUnlock = useCallback(async () => {
    if (!password) return;
    setUnlocking(true);
    setError("");
    try {
      // Init nostr node if identity exists (required for wallet unlock)
      try {
        await invoke("init_nostr_identity");
      } catch {
        // Identity may not exist — generate one for wallet access
        await invoke("generate_nostr_identity");
      }
      await invoke<void>("unlock_wallet", { password });
      useStore.setState({
        walletStatus: "unlocked",
        walletSessionPassword: password,
      });
      setPassword("");
    } catch (e) {
      const msg = String(e);
      setError(
        msg.includes("Wrong") || msg.includes("password")
          ? "Wrong password"
          : "Failed to unlock wallet",
      );
    }
    setUnlocking(false);
  }, [password]);

  return (
    <div className="space-y-3">
      <p className="text-xs text-slate-500">
        Wallet is locked. Enter your password to unlock.
      </p>
      <input
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") void handleUnlock();
        }}
        placeholder="Wallet password"
        className="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2"
        disabled={unlocking}
      />
      {error && <p className="text-xs text-rose-400">{error}</p>}
      <button
        type="button"
        onClick={handleUnlock}
        disabled={unlocking || !password}
        className="w-full rounded-lg bg-emerald-400 px-4 py-2.5 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {unlocking ? "Unlocking..." : "Unlock"}
      </button>
    </div>
  );
}

function WalletSection() {
  const walletStatus = useStore((s) => s.walletStatus);
  const nostrNpub = useStore((s) => s.nostrNpub);
  const showMiniWallet = useStore((s) => s.showMiniWallet);
  const showLbtcLabel = useStore((s) => s.showLbtcLabel);
  const baseCurrency = useStore((s) => s.baseCurrency);
  const marketMakerMode = useStore((s) => s.marketMakerMode);
  const walletDeletePrompt = useStore((s) => s.walletDeletePrompt);
  const walletDeleteConfirm = useStore((s) => s.walletDeleteConfirm);

  const canConfirmDelete =
    walletDeleteConfirm.trim().toUpperCase() === "DELETE";

  if (walletStatus === "not_created") {
    return (
      <div className="space-y-3">
        <p className="text-xs text-slate-500">
          No wallet configured on this device.
        </p>
        <button
          type="button"
          onClick={() =>
            useStore.setState({ walletOpen: true, settingsOpen: false })
          }
          className="w-full rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
        >
          Set Up Wallet
        </button>
      </div>
    );
  }

  if (walletStatus === "locked") {
    return <InlineUnlock />;
  }

  return (
    <div className="space-y-3">
      {/* Download backup file */}
      {nostrNpub && (
        <button
          type="button"
          onClick={async () => {
            try {
              const nsec = await invoke<string>("export_nostr_nsec");
              let mnemonic: string;
              try {
                mnemonic = await invoke<string>("get_cached_mnemonic");
              } catch {
                const pw = useStore.getState().walletSessionPassword;
                if (!pw) return;
                mnemonic = await invoke<string>("get_wallet_mnemonic", {
                  password: pw,
                });
              }
              const pw = useStore.getState().walletSessionPassword;
              if (!pw) return;
              const name =
                useStore.getState().nostrProfile?.display_name ||
                useStore.getState().nostrProfile?.name ||
                "unnamed";
              const fileContent = await invoke<string>("export_identity_file", {
                password: pw,
                nsec,
                mnemonic,
                displayName: name,
              });
              const blob = new Blob([fileContent], {
                type: "application/json",
              });
              const url = URL.createObjectURL(blob);
              const a = document.createElement("a");
              a.href = url;
              a.download = `deadcat-${name}.dcid`;
              a.click();
              URL.revokeObjectURL(url);
              showToast("Backup file downloaded");
            } catch (e) {
              console.warn("Backup download failed:", e);
              showToast("Backup failed", "error");
            }
          }}
          className="flex w-full items-center justify-center gap-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-4 py-3 text-sm font-medium text-emerald-300 transition hover:bg-emerald-500/20"
        >
          <svg
            aria-hidden="true"
            className="h-4 w-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth="2"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
            />
          </svg>
          Download backup file (.dcid)
        </button>
      )}
      {/* Show balance in nav bar */}
      <div className="flex items-center justify-between py-2.5">
        <div>
          <p className="text-xs text-slate-300">Show balance in nav bar</p>
          <p className="text-[10px] text-slate-500">
            Display mini wallet balance next to the wallet icon
          </p>
        </div>
        <Toggle
          enabled={showMiniWallet}
          onClick={() =>
            useStore.setState((s) => ({ showMiniWallet: !s.showMiniWallet }))
          }
        />
      </div>

      {/* L-BTC label */}
      <div className="flex items-center justify-between py-2.5">
        <div>
          <p className="text-xs text-slate-300">Show L-BTC asset label</p>
          <p className="text-[10px] text-slate-500">
            Display &quot;{btcLabel(showLbtcLabel)}&quot; instead of &quot;
            {showLbtcLabel ? "BTC" : "L-BTC"}&quot; for Liquid Bitcoin
          </p>
        </div>
        <Toggle
          enabled={showLbtcLabel}
          onClick={() =>
            useStore.setState((s) => ({ showLbtcLabel: !s.showLbtcLabel }))
          }
        />
      </div>

      {/* Display currency */}
      <div className="py-2 space-y-2">
        <p className="text-xs font-medium uppercase tracking-wider text-slate-500">
          Display Currency
        </p>
        <p className="text-[10px] text-slate-500">
          Show fiat equivalents for BTC amounts
        </p>
        <div className="grid grid-cols-3 gap-1">
          {baseCurrencyOptions.map((c) => (
            <button
              type="button"
              key={c}
              onClick={() =>
                useStore.setState({ baseCurrency: c as BaseCurrency })
              }
              className={`rounded-md px-2 py-1 text-xs transition ${c === baseCurrency ? "bg-emerald-400/15 border border-emerald-400/40 text-emerald-300" : "border border-slate-700 text-slate-400 hover:bg-slate-800 hover:text-slate-200"}`}
            >
              {c}
            </button>
          ))}
        </div>
      </div>

      {/* Market Maker mode */}
      <div className="flex items-center justify-between py-2.5">
        <div>
          <p className="text-xs text-slate-300">Market Maker mode</p>
          <p className="text-[10px] text-slate-500">
            Create markets, issue tokens, resolve as oracle
          </p>
        </div>
        <Toggle
          enabled={marketMakerMode}
          onClick={() =>
            useStore.setState((s) => ({
              marketMakerMode: !s.marketMakerMode,
            }))
          }
        />
      </div>

      {/* Recovery phrase */}
      <SeedPhraseReveal />

      {/* Remove wallet */}
      <p className="text-xs text-slate-500">
        Remove the current wallet from this device. You can restore from your
        backup file (.dcid) or recovery phrase.
      </p>
      {walletDeletePrompt ? (
        <div className="rounded-lg border border-rose-700/40 bg-rose-950/20 p-3 space-y-2">
          <p className="text-xs text-rose-300">
            This will permanently remove your wallet. Type{" "}
            <strong>DELETE</strong> to confirm.
          </p>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={walletDeleteConfirm}
              onChange={(e) =>
                useStore.setState({ walletDeleteConfirm: e.target.value })
              }
              placeholder="Type DELETE"
              className="h-9 min-w-0 flex-1 rounded-lg border border-rose-700/40 bg-slate-900 px-3 text-xs text-rose-300 outline-none ring-rose-400 transition focus:ring-2 uppercase"
              autoComplete="off"
            />
            <button
              type="button"
              onClick={async () => {
                try {
                  await invoke("delete_wallet");
                  useStore.setState({
                    walletStatus: "not_created",
                    walletData: null,
                    walletDeletePrompt: false,
                    walletDeleteConfirm: "",
                  });
                } catch {
                  /* ignore */
                }
              }}
              disabled={!canConfirmDelete}
              className={`shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${canConfirmDelete ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}`}
            >
              Continue
            </button>
            <button
              type="button"
              onClick={() =>
                useStore.setState({
                  walletDeletePrompt: false,
                  walletDeleteConfirm: "",
                })
              }
              className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => useStore.setState({ walletDeletePrompt: true })}
          className="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition"
        >
          Remove Wallet
        </button>
      )}
    </div>
  );
}

/* ── Relays section ────────────────────────────────────────────────── */

function RelaysSection() {
  const relays = useStore((s) => s.relays);
  const relayInput = useStore((s) => s.relayInput);
  const relayLoading = useStore((s) => s.relayLoading);

  const addRelay = useCallback(async () => {
    const url = useStore.getState().relayInput.trim();
    if (!url) return;
    try {
      await invoke("add_relay", { url });
      useStore.setState({ relayInput: "" });
    } catch {
      /* ignore */
    }
  }, []);

  const removeRelay = useCallback(async (url: string) => {
    try {
      await invoke("remove_relay", { url });
    } catch {
      /* ignore */
    }
  }, []);

  const resetRelays = useCallback(async () => {
    try {
      const defaults = ["wss://relay.damus.io", "wss://relay.primal.net"];
      await invoke("set_relay_list", { relays: defaults });
      useStore.setState({
        relays: defaults.map((url) => ({ url, has_backup: false })),
      });
    } catch {
      /* ignore */
    }
  }, []);

  return (
    <div className="space-y-3">
      <p className="text-xs text-slate-500">
        Nostr relays used for publishing and fetching data.
      </p>
      <div className="space-y-1.5">
        {relays.map((relay: RelayEntry) => (
          <div
            key={relay.url}
            className="flex items-center gap-2 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2"
          >
            <div className="min-w-0 flex-1 truncate text-xs text-slate-300 mono">
              {relay.url}
            </div>
            {relay.has_backup && (
              <svg
                aria-hidden="true"
                className="h-3.5 w-3.5 shrink-0 text-emerald-400"
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
            )}
            {relays.length > 1 && (
              <button
                type="button"
                onClick={() => removeRelay(relay.url)}
                className="shrink-0 text-slate-500 hover:text-rose-400 transition"
              >
                <svg
                  aria-hidden="true"
                  className="h-3.5 w-3.5"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth="2"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </button>
            )}
          </div>
        ))}
      </div>
      <div className="flex items-center gap-2">
        <input
          value={relayInput}
          onChange={(e) => useStore.setState({ relayInput: e.target.value })}
          placeholder="wss://relay.example.com"
          className="h-9 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono"
        />
        <button
          type="button"
          onClick={addRelay}
          disabled={relayLoading}
          className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
        >
          Add
        </button>
      </div>
      <button
        type="button"
        onClick={resetRelays}
        className="text-[10px] text-slate-500 hover:text-slate-300 transition"
      >
        Reset to defaults
      </button>
    </div>
  );
}

/* ── Data Source section ───────────────────────────────────────────── */

function DataSourceSection() {
  const sourceNpub = useStore((s) => s.sourceNpub);
  const sourceNpubInput = useStore((s) => s.sourceNpubInput);
  const sourceNpubSaving = useStore((s) => s.sourceNpubSaving);

  const isChanged = sourceNpubInput !== sourceNpub;

  const saveSourceNpub = useCallback(async () => {
    const npub = useStore.getState().sourceNpubInput.trim();
    if (!npub) return;
    useStore.setState({ sourceNpubSaving: true });
    try {
      await invoke("set_source_npub", { npub });
      useStore.setState({ sourceNpub: npub });
      showToast("Source npub saved — restart to apply");
    } catch (e) {
      showToast(`Invalid npub: ${e}`);
    } finally {
      useStore.setState({ sourceNpubSaving: false });
    }
  }, []);

  const resetToDefault = useCallback(async () => {
    try {
      const defaultNpub = await invoke<string>("get_default_source_npub");
      useStore.setState({ sourceNpubInput: defaultNpub });
    } catch {
      /* ignore */
    }
  }, []);

  return (
    <div className="space-y-3">
      <p className="text-xs text-slate-500">
        Markets are sourced from a single Nostr pubkey. Change this to view
        markets published by a different account.
      </p>
      <div className="space-y-2">
        <label
          htmlFor="source-npub-input"
          className="block text-[10px] font-medium uppercase tracking-wider text-slate-500"
        >
          Source npub
        </label>
        <input
          id="source-npub-input"
          value={sourceNpubInput}
          onChange={(e) =>
            useStore.setState({ sourceNpubInput: e.target.value })
          }
          placeholder="npub1..."
          className="h-9 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono"
        />
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={saveSourceNpub}
            disabled={sourceNpubSaving || !isChanged}
            className="rounded-lg border border-slate-700 px-3 py-1.5 text-xs text-slate-300 transition hover:bg-slate-800 disabled:opacity-40"
          >
            {sourceNpubSaving ? "Saving..." : "Save"}
          </button>
          <button
            type="button"
            onClick={resetToDefault}
            className="text-[10px] text-slate-500 hover:text-slate-300 transition"
          >
            Reset to default
          </button>
        </div>
      </div>
    </div>
  );
}

/* ── Dev section ───────────────────────────────────────────────────── */

function DevSection() {
  const devResetPrompt = useStore((s) => s.devResetPrompt);
  const devResetConfirm = useStore((s) => s.devResetConfirm);

  const canConfirmReset = devResetConfirm.trim().toUpperCase() === "RESET";

  return (
    <div className="space-y-2">
      <button
        type="button"
        onClick={() => {
          void queryClient.invalidateQueries({ queryKey: ["markets"] });
          useStore.setState({
            settingsOpen: false,
            activeCategory: "Trending",
          });
        }}
        className="w-full rounded-lg border border-emerald-700/40 px-4 py-2 text-xs text-emerald-400 hover:bg-emerald-900/20 transition"
      >
        Refresh Markets
      </button>
      <button
        type="button"
        onClick={() => window.location.reload()}
        className="w-full rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-400 hover:bg-slate-800 transition"
      >
        Restart App
      </button>
      {devResetPrompt ? (
        <div className="rounded-lg border border-rose-700/40 bg-rose-950/20 p-3 space-y-2">
          <p className="text-xs text-rose-300">
            This will erase your <strong>Nostr identity</strong> and{" "}
            <strong>wallet</strong>. Type <strong>RESET</strong> to confirm.
          </p>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={devResetConfirm}
              onChange={(e) =>
                useStore.setState({ devResetConfirm: e.target.value })
              }
              placeholder="Type RESET"
              className="h-9 min-w-0 flex-1 rounded-lg border border-rose-700/40 bg-slate-900 px-3 text-xs text-rose-300 outline-none ring-rose-400 transition focus:ring-2 uppercase"
              autoComplete="off"
            />
            <button
              type="button"
              onClick={async () => {
                try {
                  await invoke("delete_wallet");
                } catch {
                  /* */
                }
                try {
                  await invoke("delete_nostr_identity");
                } catch {
                  /* */
                }
                useStore.setState({
                  nostrPubkey: null,
                  nostrNpub: null,
                  nostrProfile: null,
                  nostrReplacePanel: false,
                  nostrReplacePrompt: false,
                  nostrReplaceConfirm: "",
                  walletStatus: "not_created",
                  walletData: null,
                  settingsOpen: false,
                  devResetPrompt: false,
                  devResetConfirm: "",
                });
                window.location.reload();
              }}
              disabled={!canConfirmReset}
              className={`shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${canConfirmReset ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}`}
            >
              Confirm
            </button>
            <button
              type="button"
              onClick={() =>
                useStore.setState({
                  devResetPrompt: false,
                  devResetConfirm: "",
                })
              }
              className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => useStore.setState({ devResetPrompt: true })}
          className="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition"
        >
          Erase All App Data
        </button>
      )}
    </div>
  );
}

/* ── Nostr Replace Panel ───────────────────────────────────────────── */

function NostrReplacePanel() {
  const nostrNpub = useStore((s) => s.nostrNpub);
  const nostrImportNsec = useStore((s) => s.nostrImportNsec);
  const nostrImporting = useStore((s) => s.nostrImporting);

  const generateKey = useCallback(async () => {
    try {
      const identity = await invoke<{ pubkey_hex: string; npub: string }>(
        "generate_nostr_identity",
      );
      const name = randomCatName();
      useStore.setState({
        nostrPubkey: identity.pubkey_hex,
        nostrNpub: identity.npub,
        nostrProfile: { name, display_name: name },
        nostrReplacePanel: false,
        nostrReplacePrompt: false,
        nostrReplaceConfirm: "",
      });
      // Publish kind 0 with the new name
      setTimeout(() => {
        void invoke("publish_nostr_profile", { name }).catch(() => {});
      }, 1000);
      showToast(`New identity created: ${name}`);
    } catch {
      /* ignore */
    }
  }, []);

  const importNsecAndClose = useCallback(async () => {
    const nsec = useStore.getState().nostrImportNsec.trim();
    if (!nsec) return;
    useStore.setState({ nostrImporting: true });
    try {
      const identity = await invoke<{ pubkey_hex: string; npub: string }>(
        "import_nostr_nsec",
        { nsec },
      );
      useStore.setState({
        nostrPubkey: identity.pubkey_hex,
        nostrNpub: identity.npub,
        nostrProfile: null,
        nostrImportNsec: "",
        nostrReplacePanel: false,
        nostrReplacePrompt: false,
        nostrReplaceConfirm: "",
      });
      // Fetch profile for imported identity
      try {
        const profile = await invoke<import("../../types").NostrProfile | null>(
          "fetch_nostr_profile",
        );
        if (profile) useStore.setState({ nostrProfile: profile });
      } catch {
        /* best effort */
      }
    } catch {
      /* ignore */
    } finally {
      useStore.setState({ nostrImporting: false });
    }
  }, []);

  return (
    <>
      <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
        <button
          type="button"
          onClick={() =>
            useStore.setState({
              nostrReplacePanel: false,
              nostrReplaceConfirm: "",
            })
          }
          className="flex items-center gap-1 text-sm text-slate-400 hover:text-slate-200 transition"
        >
          <svg
            aria-hidden="true"
            className="h-4 w-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth="2"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M15 19l-7-7 7-7"
            />
          </svg>
          Back
        </button>
        <CloseButton
          onClick={() => useStore.setState({ settingsOpen: false })}
        />
      </div>
      <div className="px-6 py-6 space-y-5">
        <div>
          <h2 className="text-lg font-medium text-slate-100">
            {nostrNpub ? "Replace Identity" : "Set Up Identity"}
          </h2>
          <p className="mt-1 text-sm text-slate-400">
            {nostrNpub
              ? "Import a different identity or generate a new one."
              : "Import an existing key or generate a new one."}
          </p>
        </div>

        <div className="space-y-2">
          <p className="text-xs text-slate-500">Import nsec</p>
          <div className="flex items-center gap-2">
            <input
              type="password"
              value={nostrImportNsec}
              onChange={(e) =>
                useStore.setState({ nostrImportNsec: e.target.value })
              }
              placeholder="nsec1..."
              className="h-10 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 transition focus:ring-2 mono"
            />
            <button
              type="button"
              onClick={importNsecAndClose}
              disabled={nostrImporting}
              className="shrink-0 rounded-lg border border-slate-700 px-3 py-2.5 text-xs text-slate-300 hover:bg-slate-800 transition"
            >
              {nostrImporting ? "Importing..." : "Import"}
            </button>
          </div>
        </div>

        <div className="border-t border-slate-800 pt-4 space-y-3">
          <p className="text-xs text-amber-300/70">
            Generating a new keypair will permanently replace your current
            identity. Make sure you have a backup.
          </p>
          <button
            type="button"
            onClick={generateKey}
            className="w-full rounded-lg border border-slate-700 px-4 py-2.5 text-sm text-slate-300 hover:bg-slate-800 transition"
          >
            Generate new keypair
          </button>
        </div>
      </div>
    </>
  );
}

/* ── Main Settings Panel ───────────────────────────────────────────── */

export function SettingsPanel() {
  const settingsOpen = useStore((s) => s.settingsOpen);
  const nostrReplacePanel = useStore((s) => s.nostrReplacePanel);

  if (!settingsOpen) return null;

  return <SettingsPanelContent nostrReplacePanel={nostrReplacePanel} />;
}

function SettingsPanelContent({
  nostrReplacePanel,
}: {
  nostrReplacePanel: boolean;
}) {
  useLockScroll();

  return (
    <div className="macos-overlay-safe-top fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm py-8">
      <div className="relative mx-4 flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl my-auto">
        {nostrReplacePanel ? (
          <NostrReplacePanel />
        ) : (
          <>
            <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
              <h2 className="text-lg font-medium text-slate-100">Settings</h2>
              <CloseButton
                onClick={() => useStore.setState({ settingsOpen: false })}
              />
            </div>
            <div className="flex-1 overflow-y-auto px-6 py-4 space-y-0">
              <SettingsAccordion sectionKey="nostr" title="Nostr Identity">
                <NostrSection />
              </SettingsAccordion>
              <SettingsAccordion sectionKey="wallet" title="Wallet">
                <WalletSection />
              </SettingsAccordion>
              <SettingsAccordion sectionKey="relays" title="Relays">
                <RelaysSection />
              </SettingsAccordion>
              <SettingsAccordion sectionKey="dataSource" title="Data Source">
                <DataSourceSection />
              </SettingsAccordion>
              {DEV_MODE && (
                <SettingsAccordion sectionKey="dev" title="Dev">
                  <DevSection />
                </SettingsAccordion>
              )}
            </div>
            <VersionFooter />
          </>
        )}
      </div>
    </div>
  );
}

function VersionFooter() {
  const [display, setDisplay] = useState<string>("");

  useEffect(() => {
    invoke<{ display: string }>("get_app_version")
      .then((v) => setDisplay(v.display))
      .catch(() => setDisplay(""));
  }, []);

  if (!display) return null;

  return (
    <div className="flex items-center justify-center gap-4 border-t border-slate-800 px-6 py-3 text-slate-600">
      <svg
        aria-hidden="true"
        viewBox="0 0 260 267"
        className="h-4 w-4 shrink-0"
        fill="currentColor"
        xmlns="http://www.w3.org/2000/svg"
      >
        <path d="M0.146484 9.04596C0.146625 1.23444 10.9146 -3.15996 16.7881 2.6983L86.5566 71.7335C100.141 68.0293 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0305 173.453 71.7364L243.212 2.71197C249.085 -3.1467 259.853 1.24702 259.854 9.05865V161.26C259.949 162.835 260 164.419 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C4.78859e-06 164.419 0.0506708 162.835 0.146484 161.26V9.04596ZM100.287 187.013L120.892 207.087V208.903C120.892 217.907 114.199 225.23 105.974 225.231H91.0049C87.1409 225.231 84.0001 228.319 84 232.118C84 235.918 87.1446 239.013 91.0049 239.013H105.974C114.534 239.013 122.574 235.049 128.02 228.383C133.461 235.045 141.502 239.013 150.065 239.013C166.019 239.013 179 225.506 179 208.903C179 205.104 175.856 202.013 171.992 202.013C168.128 202.013 164.984 205.104 164.983 208.903C164.983 217.907 158.291 225.231 150.065 225.231C141.84 225.23 135.147 217.907 135.147 208.903V207.049L155.713 187.013H100.287ZM70.4697 140.12L52.4219 122.072L44 130.495L62.0469 148.542L44.0596 166.53L52.4824 174.953L70.4697 156.965L88.5176 175.013L96.9404 166.591L78.8916 148.542L97 130.435L88.5781 122.013L70.4697 140.12ZM195.367 123.557C200.554 128.783 204 138.006 204 148.513C204 158.3 201.01 166.973 196.408 172.339C216.243 169.73 231 159.83 231 148.013C231 135.99 215.724 125.951 195.367 123.557ZM175.489 123.7C155.707 126.33 141 136.216 141 148.013C141 159.603 155.197 169.349 174.456 172.181C169.931 166.803 167 158.204 167 148.513C167 138.102 170.382 128.951 175.489 123.7Z" />
      </svg>
      <span className="text-xs">v{display}</span>
      <button
        type="button"
        onClick={() => void openUrl("https://github.com/Resolvr-io/deadcat")}
        className="flex items-center gap-1.5 text-xs text-slate-600 transition hover:text-slate-400"
      >
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          className="h-3.5 w-3.5 shrink-0"
          fill="currentColor"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.56 0-.27-.01-1-.02-1.96-3.2.7-3.87-1.54-3.87-1.54-.52-1.33-1.28-1.68-1.28-1.68-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.18 1.76 1.18 1.03 1.76 2.7 1.25 3.36.96.1-.75.4-1.25.73-1.54-2.55-.29-5.23-1.28-5.23-5.7 0-1.26.45-2.29 1.18-3.1-.12-.29-.51-1.47.11-3.06 0 0 .97-.31 3.18 1.18a11 11 0 0 1 5.79 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.77.11 3.06.73.81 1.18 1.84 1.18 3.1 0 4.43-2.69 5.41-5.25 5.69.41.36.78 1.06.78 2.14 0 1.55-.02 2.79-.02 3.17 0 .31.21.68.8.56C20.21 21.38 23.5 17.08 23.5 12 23.5 5.65 18.35.5 12 .5z" />
        </svg>
        <span>Report bugs or contribute</span>
      </button>
    </div>
  );
}
