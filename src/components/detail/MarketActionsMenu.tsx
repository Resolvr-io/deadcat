import { openUrl } from "@tauri-apps/plugin-opener";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useEscapeKey } from "../../hooks/useEscapeKey";
import { useStore } from "../../store";

type MarketLike = {
  nevent?: string;
  nostrEventJson?: string;
  creationTxid?: string;
};

/**
 * Kebab "more actions" menu pinned to the top-right of the market
 * header. Hides secondary links — Nostr event JSON viewer and the
 * block-explorer creation transaction — behind a small icon so the
 * header's primary metadata (category, state) stays the visual focus.
 *
 * Accepts any object with nevent / nostrEventJson / creationTxid so it
 * works for both binary Market and MarketGroup.
 */
export function MarketActionsMenu({ market }: { market: MarketLike }) {
  const walletNetwork = useStore((s) => s.walletNetwork);
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement | null>(null);

  useEscapeKey(open, () => setOpen(false));

  useEffect(() => {
    if (!open) return;
    function handleClickOutside(e: MouseEvent) {
      if (
        wrapperRef.current &&
        !wrapperRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  const close = () => setOpen(false);

  const handleNostrEvent = () => {
    useStore.setState({
      nostrEventModal: true,
      nostrEventJson: market.nostrEventJson ?? null,
      nostrEventNevent: market.nevent ?? null,
    });
    close();
  };

  const handleCreationTx = () => {
    if (!market.creationTxid) return;
    const base =
      walletNetwork === "testnet"
        ? "https://blockstream.info/liquidtestnet"
        : "https://blockstream.info/liquid";
    void openUrl(`${base}/tx/${market.creationTxid}`);
    close();
  };

  return (
    <div ref={wrapperRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        title="More"
        aria-haspopup="menu"
        aria-expanded={open}
        className="flex h-7 w-7 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800/60 hover:text-slate-200"
      >
        <svg
          aria-hidden="true"
          className="h-[18px] w-[18px]"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <circle cx="12" cy="12" r="1" />
          <circle cx="19" cy="12" r="1" />
          <circle cx="5" cy="12" r="1" />
        </svg>
      </button>

      {open && (
        <div
          role="menu"
          className="absolute right-0 top-full z-20 mt-1 w-56 overflow-hidden rounded-lg border border-slate-700 bg-slate-900 shadow-xl"
        >
          <MenuItem
            label="View Nostr event"
            icon={
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
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                <polyline points="14 2 14 8 20 8" />
              </svg>
            }
            onClick={handleNostrEvent}
          />
          {market.creationTxid && (
            <MenuItem
              label="Creation transaction"
              icon={
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
                  <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
                  <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
                </svg>
              }
              onClick={handleCreationTx}
            />
          )}
        </div>
      )}
    </div>
  );
}

function MenuItem({
  icon,
  label,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      className="flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm text-slate-200 transition hover:bg-slate-800"
    >
      <span className="shrink-0 text-slate-500">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
    </button>
  );
}
