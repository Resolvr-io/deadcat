import { useCallback, useEffect, useMemo, useState } from "react";
import { useEscapeKey } from "../../../hooks/useEscapeKey";
import { useLockScroll } from "../../../hooks/useLockScroll";
import { useBcConnection } from "../../../queries/useBcConnection";
import { useNostrProfileByPubkey } from "../../../queries/useNostrProfileByPubkey";
import { useZapPrefs } from "../../../queries/useZapPrefs";
import { useStore } from "../../../store";
import { friendlyError } from "../../../utils-react/friendly-error";
import { sendZap } from "../../../utils-react/zap";
import { CloseButton } from "../../shared/CloseButton";
import { SigningHint } from "../../shared/SigningHint";
import { showToast } from "../../shared/Toast";
import { ZapIcon } from "./icons";

const AMOUNT_PRESETS = [21, 100, 500, 1000, 5000] as const;

export function ZapDialog({
  recipientPubkeyHex,
  eventIdHex,
  open,
  onClose,
  onZapped,
}: {
  recipientPubkeyHex: string;
  /** Hex event id when zapping a specific comment/note; omit for profile zaps. */
  eventIdHex?: string;
  open: boolean;
  onClose: () => void;
  /** Fires once the payment settles via the BC provider. Callers use
   *  this to bump optimistic UI counters. */
  onZapped?: () => void;
}) {
  useLockScroll(open);
  useEscapeKey(open, onClose);

  const { defaultSats, defaultComment } = useZapPrefs();
  const { connected } = useBcConnection();
  const { data: profile, isLoading: profileLoading } = useNostrProfileByPubkey(
    open ? recipientPubkeyHex : null,
  );
  const relayEntries = useStore((s) => s.relays);
  const relayUrls = useMemo(
    () => relayEntries.map((r) => r.url).filter(Boolean),
    [relayEntries],
  );

  const [amount, setAmount] = useState<number>(defaultSats);
  const [comment, setComment] = useState<string>(defaultComment);
  const [zapping, setZapping] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset transient state when the dialog opens.
  useEffect(() => {
    if (!open) return;
    setAmount(defaultSats);
    setComment(defaultComment);
    setZapping(false);
    setError(null);
  }, [open, defaultSats, defaultComment]);

  const hasLightningAddress = !!profile?.lud16;
  const recipientName =
    profile?.display_name?.trim() || profile?.name?.trim() || "this user";

  const handleZap = useCallback(async () => {
    if (!profile) return;
    if (amount <= 0) {
      setError("Enter an amount greater than zero");
      return;
    }
    setZapping(true);
    setError(null);
    try {
      await sendZap({
        recipientProfile: profile,
        recipientPubkeyHex,
        amountSats: amount,
        content: comment.trim(),
        relays: relayUrls,
        eventIdHex,
      });
      showToast(`Zapped ${amount.toLocaleString()} sats`, "success");
      onZapped?.();
      onClose();
    } catch (e) {
      setError(friendlyError(String(e)));
    } finally {
      setZapping(false);
    }
  }, [
    profile,
    recipientPubkeyHex,
    amount,
    comment,
    relayUrls,
    eventIdHex,
    onClose,
    onZapped,
  ]);

  if (!open) return null;

  return (
    <div
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    >
      <div className="relative mx-4 w-full max-w-md overflow-hidden rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-800 px-5 py-4">
          <div className="flex items-center gap-2">
            <div className="flex h-7 w-7 items-center justify-center rounded-full bg-amber-400/15">
              <ZapIcon className="h-4 w-4 text-amber-300" />
            </div>
            <h2 className="text-base font-semibold text-slate-100">
              Zap {recipientName}
            </h2>
          </div>
          <CloseButton onClick={onClose} />
        </div>

        <div className="space-y-4 px-5 py-4">
          {profileLoading ? (
            <p className="py-6 text-center text-xs text-slate-500">
              Loading profile…
            </p>
          ) : !hasLightningAddress ? (
            <p className="rounded-lg border border-slate-800 bg-slate-900/60 px-3 py-3 text-xs text-slate-400">
              {recipientName} doesn't have a Lightning address in their profile
              yet, so they can't receive zaps.
            </p>
          ) : (
            <>
              <div>
                <p className="mb-2 text-xs font-medium uppercase tracking-wide text-slate-500">
                  Amount
                </p>
                <div className="mb-2 flex flex-wrap gap-1.5">
                  {AMOUNT_PRESETS.map((preset) => (
                    <button
                      key={preset}
                      type="button"
                      onClick={() => setAmount(preset)}
                      className={`rounded-full px-3 py-1 text-xs transition ${
                        amount === preset
                          ? "bg-emerald-400 text-slate-950"
                          : "border border-slate-700 text-slate-300 hover:bg-slate-800"
                      }`}
                    >
                      {preset.toLocaleString()}
                    </button>
                  ))}
                </div>
                <div className="flex items-center gap-2">
                  <input
                    type="number"
                    inputMode="numeric"
                    min={1}
                    value={amount}
                    onChange={(e) => {
                      const n = Number.parseInt(e.target.value, 10);
                      setAmount(
                        Number.isFinite(n) && n > 0 ? Math.floor(n) : 0,
                      );
                    }}
                    className="h-10 flex-1 rounded-md border border-slate-700 bg-slate-900 px-3 text-sm text-slate-100 focus:border-slate-600 focus:outline-none focus:ring-2 focus:ring-emerald-400/30"
                  />
                  <span className="text-xs text-slate-500">sats</span>
                </div>
              </div>

              <div>
                <p className="mb-2 text-xs font-medium uppercase tracking-wide text-slate-500">
                  Message (optional)
                </p>
                <input
                  type="text"
                  maxLength={140}
                  placeholder="Add a short message"
                  value={comment}
                  onChange={(e) => setComment(e.target.value)}
                  className="h-10 w-full rounded-md border border-slate-700 bg-slate-900 px-3 text-sm text-slate-100 placeholder:text-slate-500 focus:border-slate-600 focus:outline-none focus:ring-2 focus:ring-emerald-400/30"
                />
              </div>

              {!connected && (
                <p className="rounded-lg border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-xs text-amber-200/80">
                  No Lightning wallet attached. A payment window will open when
                  you confirm.
                </p>
              )}

              <SigningHint active={zapping} />

              {error && (
                <p className="rounded-lg border border-rose-500/30 bg-rose-500/5 px-3 py-2 text-xs text-rose-300">
                  {error}
                </p>
              )}
            </>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-slate-800 px-5 py-3">
          <button
            type="button"
            onClick={onClose}
            disabled={zapping}
            className="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 transition hover:bg-slate-800 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void handleZap()}
            disabled={!hasLightningAddress || zapping || amount <= 0}
            className="flex items-center gap-1.5 rounded-lg bg-amber-400 px-4 py-2 text-sm font-semibold text-slate-950 transition hover:bg-amber-300 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <ZapIcon className="h-4 w-4" />
            {zapping ? "Zapping…" : `Zap ${amount.toLocaleString()} sats`}
          </button>
        </div>
      </div>
    </div>
  );
}
