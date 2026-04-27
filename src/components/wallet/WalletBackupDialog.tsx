import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { useEscapeKey } from "../../hooks/useEscapeKey";
import { useLockScroll } from "../../hooks/useLockScroll";
import { setWalletNeedsBackup } from "../../hooks/useWalletNeedsBackup";
import { useStore } from "../../store";
import { CloseButton } from "../shared/CloseButton";
import { showToast } from "../shared/Toast";

function MnemonicGrid({ mnemonic }: { mnemonic: string }) {
  const words = mnemonic.split(/\s+/).filter(Boolean);
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
              className="flex min-w-0 items-baseline gap-1.5"
            >
              <span className="shrink-0 text-xs text-slate-500">
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

export function WalletBackupDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const sessionPassword = useStore((s) => s.walletSessionPassword);
  const [mnemonic, setMnemonic] = useState<string>("");
  const [password, setPassword] = useState("");
  const [passwordError, setPasswordError] = useState("");
  const [revealing, setRevealing] = useState(false);

  useLockScroll(open);
  useEscapeKey(open, onClose);

  // Auto-reveal when the session password is known (user is unlocked).
  useEffect(() => {
    if (!open) return;
    if (mnemonic) return;
    if (!sessionPassword) return;
    let cancelled = false;
    void (async () => {
      try {
        const m = await invoke<string>("get_wallet_mnemonic", {
          password: sessionPassword,
        });
        if (!cancelled) setMnemonic(m);
      } catch {
        // Session password invalid — fall back to prompt.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, mnemonic, sessionPassword]);

  // Reset transient state when the dialog closes.
  useEffect(() => {
    if (open) return;
    setMnemonic("");
    setPassword("");
    setPasswordError("");
    setRevealing(false);
  }, [open]);

  const handleReveal = useCallback(async () => {
    if (!password) return;
    setRevealing(true);
    setPasswordError("");
    try {
      const m = await invoke<string>("get_wallet_mnemonic", { password });
      setMnemonic(m);
    } catch {
      setPasswordError("Incorrect password");
    }
    setRevealing(false);
  }, [password]);

  const handleCopy = useCallback(() => {
    if (!mnemonic) return;
    void navigator.clipboard.writeText(mnemonic);
    showToast("Copied recovery phrase to clipboard");
  }, [mnemonic]);

  const handleConfirm = useCallback(() => {
    setWalletNeedsBackup(false);
    onClose();
  }, [onClose]);

  if (!open) return null;

  return (
    <div
      role="presentation"
      className="macos-overlay-safe-top fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="relative mx-4 w-full max-w-md rounded-2xl border border-slate-700 bg-slate-950 p-8 shadow-2xl">
        <CloseButton
          onClick={onClose}
          className="absolute right-4 top-4"
          variant="overlay"
        />
        <p className="mb-2 text-xs font-semibold uppercase tracking-widest text-amber-300">
          Wallet backup
        </p>
        <h2 className="text-2xl font-semibold text-white">
          Back up your recovery phrase
        </h2>
        <p className="mt-3 text-sm leading-relaxed text-slate-400">
          Write these 12 words down in order and store them somewhere safe. This
          is the only way to recover your wallet if you lose this device.
        </p>

        {mnemonic ? (
          <>
            <div className="mt-6 space-y-4 rounded-xl border border-slate-700 bg-slate-900/60 p-5">
              <MnemonicGrid mnemonic={mnemonic} />
              <button
                type="button"
                onClick={handleCopy}
                className="w-full rounded-lg border border-slate-700 px-4 py-2.5 text-sm text-slate-300 transition hover:bg-slate-800"
              >
                Copy to clipboard
              </button>
            </div>
            <button
              type="button"
              onClick={handleConfirm}
              className="mt-6 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 transition hover:bg-emerald-300"
            >
              I have saved my recovery phrase
            </button>
          </>
        ) : (
          <div className="mt-6 space-y-4">
            <div>
              <label
                htmlFor="wallet-backup-password"
                className="text-xs font-medium uppercase tracking-wide text-slate-400"
              >
                Password
              </label>
              <input
                id="wallet-backup-password"
                type="password"
                autoComplete="current-password"
                value={password}
                onChange={(e) => {
                  setPassword(e.target.value);
                  if (passwordError) setPasswordError("");
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void handleReveal();
                }}
                placeholder="Wallet password"
                className="mt-1.5 h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2"
              />
              {passwordError && (
                <p className="mt-2 text-xs text-red-400">{passwordError}</p>
              )}
            </div>
            <button
              type="button"
              onClick={handleReveal}
              disabled={!password || revealing}
              className="w-full rounded-lg bg-emerald-400 px-4 py-3 font-semibold text-slate-950 transition hover:bg-emerald-300 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {revealing ? "Unlocking…" : "Reveal recovery phrase"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
