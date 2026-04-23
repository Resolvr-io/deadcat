import { openUrl } from "@tauri-apps/plugin-opener";
import { useEscapeKey } from "../../../hooks/useEscapeKey";
import { useLockScroll } from "../../../hooks/useLockScroll";
import { CloseButton } from "../../shared/CloseButton";

export function LinkConfirmDialog({
  url,
  open,
  onClose,
}: {
  url: string;
  open: boolean;
  onClose: () => void;
}) {
  useLockScroll(open);
  useEscapeKey(open, onClose);

  if (!open) return null;

  const handleOpen = () => {
    void openUrl(url);
    onClose();
  };

  return (
    <div
      role="presentation"
      className="macos-overlay-safe-top fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="relative mx-4 w-full max-w-md rounded-2xl border border-slate-700 bg-slate-950 p-6 shadow-2xl">
        <CloseButton onClick={onClose} className="absolute right-4 top-4" />
        <p className="mb-2 text-xs font-semibold uppercase tracking-widest text-amber-300">
          External link
        </p>
        <h2 className="pr-8 text-lg font-semibold text-white">
          Open this link in your browser?
        </h2>
        <p className="mt-3 text-xs leading-relaxed text-slate-400">
          Deadcat won&apos;t open URLs without your confirmation. Verify the
          destination before continuing.
        </p>
        <div className="mt-4 overflow-x-auto rounded-lg border border-slate-700 bg-slate-900/60 px-3 py-2.5">
          <span className="mono break-all text-xs text-slate-200">{url}</span>
        </div>
        <div className="mt-6 flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 transition hover:bg-slate-800"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleOpen}
            className="rounded-lg bg-emerald-400 px-4 py-2 text-sm font-semibold text-slate-950 transition hover:bg-emerald-300"
          >
            Open in browser
          </button>
        </div>
      </div>
    </div>
  );
}
