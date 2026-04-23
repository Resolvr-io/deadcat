import { useEscapeKey } from "../../hooks/useEscapeKey";
import { useLockScroll } from "../../hooks/useLockScroll";
import { CloseButton } from "../shared/CloseButton";

/**
 * Explains the split between the onboard Liquid wallet and an
 * external Bitcoin Connect / NWC Lightning wallet. Surfaced from a
 * small "What's this?" link next to the Connect button in Settings →
 * Lightning wallet.
 */
export function LightningWalletInfoDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  useLockScroll(open);
  useEscapeKey(open, onClose);

  if (!open) return null;

  return (
    <div
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    >
      <div className="relative mx-4 w-full max-w-lg overflow-hidden rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
          <h2 className="text-base font-semibold text-slate-100">
            Two wallets, one app
          </h2>
          <CloseButton onClick={onClose} />
        </div>
        <div className="max-h-[70vh] space-y-5 overflow-y-auto px-6 py-5 text-sm leading-relaxed text-slate-300">
          <p>
            Deadcat splits your funds across{" "}
            <strong>two independent wallets</strong> that you fully control.
          </p>

          <section>
            <h3 className="mb-1.5 text-sm font-semibold text-emerald-300">
              Liquid Bitcoin Wallet
            </h3>
            <p>
              Self-custodied L-BTC held by this app. It backs every
              prediction-market position you take — collateral, token issuance,
              resolution, and redemption all settle on the Liquid network. Your
              seed is encrypted on this device and never leaves it.
            </p>
          </section>

          <section>
            <h3 className="mb-1.5 text-sm font-semibold text-amber-300">
              Lightning Wallet
            </h3>
            <p>
              A <strong>separate</strong> Lightning wallet you already own,
              attached via <span className="mono text-xs">NWC</span> (Nostr
              Wallet Connect). Deadcat never holds your Lightning keys — it asks
              your wallet to pay invoices on your behalf. Works with Alby Hub,
              Coinos, Flash, Rizful, and any other NWC-compatible wallet.
            </p>
            <p className="mt-2">
              Used for <strong>zapping comments and profiles</strong> and
              (coming soon) moving value between your Liquid balance and
              Lightning via an on-demand Boltz swap.
            </p>
          </section>

          <section className="rounded-lg border border-slate-800 bg-slate-900/60 p-3">
            <p className="text-xs text-slate-400">
              <strong className="text-slate-200">
                Why keep them separate?
              </strong>{" "}
              Liquid is great for self-custodied on-chain value; Lightning is
              great for instant tips and payments. Keeping the two as distinct
              wallets means you decide when — and how much — to move between
              them, instead of one balance silently bleeding into the other.
            </p>
          </section>
        </div>
        <div className="flex items-center justify-end gap-2 border-t border-slate-800 px-6 py-3">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg bg-emerald-400 px-4 py-2 text-sm font-semibold text-slate-950 transition hover:bg-emerald-300"
          >
            Got it
          </button>
        </div>
      </div>
    </div>
  );
}
