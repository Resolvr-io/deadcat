import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import { baseCurrencyOptions } from "../../constants";
import { queryClient } from "../../queries/queryClient";
import { useStore } from "../../store";
import type { BaseCurrency, RelayEntry } from "../../types";
import { btcLabel } from "../../utils-react/wallet";
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
    <div className="rounded-lg border border-slate-800 overflow-hidden">
      <button
        type="button"
        onClick={toggle}
        className="w-full flex items-center justify-between px-4 py-3 text-left transition-colors hover:bg-slate-900/50"
      >
        <span className="text-xs font-medium uppercase tracking-wider text-slate-400">
          {title}
        </span>
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
      {open && (
        <div className="px-4 pb-4 pt-3 border-t border-slate-800">
          {children}
        </div>
      )}
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
  const nostrImportNsec = useStore((s) => s.nostrImportNsec);
  const nostrImporting = useStore((s) => s.nostrImporting);
  const nostrReplacePrompt = useStore((s) => s.nostrReplacePrompt);
  const nostrReplaceConfirm = useStore((s) => s.nostrReplaceConfirm);

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
      showToast("Copied nsec to clipboard");
    }
  }, []);

  const revealNsec = useCallback(async () => {
    try {
      const nsec = await invoke<string>("export_nostr_nsec");
      useStore.setState({ nostrNsecRevealed: nsec });
    } catch {
      /* ignore */
    }
  }, []);

  const importNsec = useCallback(async () => {
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
        nostrImportNsec: "",
        nostrReplacePanel: false,
        nostrReplacePrompt: false,
        nostrReplaceConfirm: "",
      });
    } catch {
      /* ignore */
    } finally {
      useStore.setState({ nostrImporting: false });
    }
  }, []);

  const generateKey = useCallback(async () => {
    try {
      const identity = await invoke<{ pubkey_hex: string; npub: string }>(
        "generate_nostr_identity",
      );
      useStore.setState({
        nostrPubkey: identity.pubkey_hex,
        nostrNpub: identity.npub,
        nostrReplacePanel: false,
        nostrReplacePrompt: false,
        nostrReplaceConfirm: "",
      });
    } catch {
      /* ignore */
    }
  }, []);

  const canConfirmReplace =
    nostrReplaceConfirm.trim().toUpperCase() === "DELETE";

  return (
    <div className="space-y-3">
      <p className="text-xs text-slate-500">
        Used to publish markets and oracle attestations on Nostr.
      </p>
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <div className="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
            <div className="text-[10px] text-slate-500">npub (public)</div>
            <div className="mono truncate text-xs text-slate-300">
              {nostrNpub ?? "Not initialized"}
            </div>
          </div>
          {nostrNpub && (
            <button
              type="button"
              onClick={copyNpub}
              className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
            >
              Copy
            </button>
          )}
        </div>

        {nostrNpub && (
          <div className="flex items-center gap-2">
            <div className="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
              <div className="text-[10px] text-slate-500">nsec (secret)</div>
              {nostrNsecRevealed ? (
                <div className="mono truncate text-xs text-rose-300">
                  {nostrNsecRevealed}
                </div>
              ) : (
                <div className="text-xs text-slate-500">Hidden</div>
              )}
            </div>
            {nostrNsecRevealed ? (
              <button
                type="button"
                onClick={copyNsec}
                className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
              >
                Copy
              </button>
            ) : (
              <button
                type="button"
                onClick={revealNsec}
                className="shrink-0 rounded-lg border border-amber-700/60 bg-amber-950/20 px-3 py-2 text-xs text-amber-300 hover:bg-amber-900/30 transition"
              >
                Reveal
              </button>
            )}
          </div>
        )}
      </div>

      {nostrNpub && (
        <div className="rounded-lg border border-amber-700/40 bg-amber-950/20 px-3 py-2">
          <p className="text-xs text-amber-300/90">
            Back up your nsec. You need it to resolve markets you create.
          </p>
        </div>
      )}

      {!nostrNpub ? (
        <div className="space-y-3">
          <div>
            <p className="text-[10px] font-medium uppercase tracking-wider text-slate-500">
              Import existing nsec
            </p>
            <div className="mt-1 flex items-center gap-2">
              <input
                type="password"
                value={nostrImportNsec}
                onChange={(e) =>
                  useStore.setState({ nostrImportNsec: e.target.value })
                }
                placeholder="nsec1..."
                className="h-9 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono"
              />
              <button
                type="button"
                onClick={importNsec}
                disabled={nostrImporting}
                className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
              >
                {nostrImporting ? "Importing..." : "Import"}
              </button>
            </div>
          </div>
          <div className="border-t border-slate-800 pt-3">
            <p className="text-[10px] font-medium uppercase tracking-wider text-slate-500">
              Or generate a fresh keypair
            </p>
            <button
              type="button"
              onClick={generateKey}
              className="mt-1 w-full rounded-lg bg-emerald-400 px-4 py-2.5 text-sm font-medium text-slate-950 hover:bg-emerald-300 transition"
            >
              Generate New Keypair
            </button>
          </div>
        </div>
      ) : nostrReplacePrompt ? (
        <div className="rounded-lg border border-rose-700/40 bg-rose-950/20 p-3 space-y-2">
          <p className="text-xs text-rose-300">
            This will permanently erase your current Nostr identity. Type{" "}
            <strong>DELETE</strong> to confirm.
          </p>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={nostrReplaceConfirm}
              onChange={(e) =>
                useStore.setState({ nostrReplaceConfirm: e.target.value })
              }
              placeholder="Type DELETE"
              className="h-9 min-w-0 flex-1 rounded-lg border border-rose-700/40 bg-slate-900 px-3 text-xs text-rose-300 outline-none ring-rose-400 transition focus:ring-2 uppercase"
              autoComplete="off"
            />
            <button
              type="button"
              onClick={() =>
                useStore.setState({
                  nostrReplacePrompt: false,
                  nostrReplacePanel: true,
                })
              }
              disabled={!canConfirmReplace}
              className={`shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${canConfirmReplace ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}`}
            >
              Continue
            </button>
            <button
              type="button"
              onClick={() =>
                useStore.setState({
                  nostrReplacePrompt: false,
                  nostrReplaceConfirm: "",
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
          onClick={() => useStore.setState({ nostrReplacePrompt: true })}
          className="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition"
        >
          Replace Nostr Keys
        </button>
      )}
    </div>
  );
}

/* ── Wallet section ────────────────────────────────────────────────── */

function SeedPhraseReveal() {
  const [revealed, setRevealed] = useState(false);
  const [words, setWords] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const handleReveal = useCallback(async () => {
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
  }, []);

  const handleCopy = useCallback(() => {
    if (words.length > 0) {
      void navigator.clipboard.writeText(words.join(" "));
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
            disabled={loading}
            className="w-full rounded-lg border border-amber-700/40 px-4 py-2 text-xs text-amber-300 hover:bg-amber-900/20 transition disabled:opacity-50"
          >
            {loading ? "Decrypting..." : "Reveal Recovery Phrase"}
          </button>
        </>
      )}
    </div>
  );
}

function WalletSection() {
  const walletStatus = useStore((s) => s.walletStatus);
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
          onClick={() => useStore.setState({ walletOpen: true })}
          className="w-full rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
        >
          Set Up Wallet
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {/* Download backup file */}
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
            const blob = new Blob([fileContent], { type: "application/json" });
            const url = URL.createObjectURL(blob);
            const a = document.createElement("a");
            a.href = url;
            a.download = `deadcat-${name}.dcid`;
            a.click();
            URL.revokeObjectURL(url);
          } catch (e) {
            console.warn("Backup download failed:", e);
          }
        }}
        className="flex w-full items-center justify-center gap-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-4 py-2.5 text-xs font-medium text-emerald-300 transition hover:bg-emerald-500/20"
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

      {/* Show balance in nav bar */}
      <div className="flex items-center justify-between rounded-lg border border-slate-700 bg-slate-900/50 px-3 py-2.5">
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
      <div className="flex items-center justify-between rounded-lg border border-slate-700 bg-slate-900/50 px-3 py-2.5">
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
      <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-3 space-y-2">
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
      <div className="flex items-center justify-between rounded-lg border border-slate-700 bg-slate-900/50 px-3 py-2.5">
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

  const importNsec = useCallback(async () => {
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
        nostrImportNsec: "",
        nostrReplacePanel: false,
        nostrReplacePrompt: false,
        nostrReplaceConfirm: "",
      });
    } catch {
      /* ignore */
    } finally {
      useStore.setState({ nostrImporting: false });
    }
  }, []);

  const generateKey = useCallback(async () => {
    try {
      const identity = await invoke<{ pubkey_hex: string; npub: string }>(
        "generate_nostr_identity",
      );
      useStore.setState({
        nostrPubkey: identity.pubkey_hex,
        nostrNpub: identity.npub,
        nostrReplacePanel: false,
        nostrReplacePrompt: false,
        nostrReplaceConfirm: "",
      });
    } catch {
      /* ignore */
    }
  }, []);

  return (
    <>
      <div className="flex items-center justify-between">
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
        <button
          type="button"
          onClick={() => useStore.setState({ settingsOpen: false })}
          className="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
        >
          <svg
            aria-hidden="true"
            className="h-5 w-5"
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
      </div>
      <div className="mt-5 space-y-5">
        <div>
          <p className="text-xs font-medium uppercase tracking-wider text-slate-500">
            {nostrNpub ? "Replace Nostr Keys" : "Set Up Nostr Identity"}
          </p>
          <p className="mt-1 text-xs text-slate-400">
            {nostrNpub
              ? "Your current identity will be permanently deleted. Choose how to set up your new identity."
              : "Import an existing key or generate a new one."}
          </p>
        </div>
        <div className="space-y-3">
          <p className="text-[10px] font-medium uppercase tracking-wider text-slate-500">
            Import existing nsec
          </p>
          <div className="flex items-center gap-2">
            <input
              type="password"
              value={nostrImportNsec}
              onChange={(e) =>
                useStore.setState({ nostrImportNsec: e.target.value })
              }
              placeholder="nsec1..."
              className="h-9 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono"
            />
            <button
              type="button"
              onClick={importNsec}
              disabled={nostrImporting}
              className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
            >
              {nostrImporting ? "Importing..." : "Import"}
            </button>
          </div>
        </div>
        <div className="border-t border-slate-800 pt-4 space-y-3">
          <p className="text-[10px] font-medium uppercase tracking-wider text-slate-500">
            Or generate a fresh keypair
          </p>
          <button
            type="button"
            onClick={generateKey}
            className="w-full rounded-lg bg-emerald-400 px-4 py-2.5 text-sm font-medium text-slate-950 hover:bg-emerald-300 transition"
          >
            Generate New Keypair
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

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-slate-950/80 backdrop-blur-sm py-8">
      <div className="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8 my-auto">
        {nostrReplacePanel ? (
          <NostrReplacePanel />
        ) : (
          <>
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-medium text-slate-100">Settings</h2>
              <button
                type="button"
                onClick={() => useStore.setState({ settingsOpen: false })}
                className="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
              >
                <svg
                  aria-hidden="true"
                  className="h-5 w-5"
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
            </div>
            <div className="mt-3 space-y-2">
              <SettingsAccordion sectionKey="nostr" title="Nostr Identity">
                <NostrSection />
              </SettingsAccordion>
              <SettingsAccordion sectionKey="wallet" title="Wallet">
                <WalletSection />
              </SettingsAccordion>
              <SettingsAccordion sectionKey="relays" title="Relays">
                <RelaysSection />
              </SettingsAccordion>
              {DEV_MODE && (
                <SettingsAccordion sectionKey="dev" title="Dev">
                  <DevSection />
                </SettingsAccordion>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
